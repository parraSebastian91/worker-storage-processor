use std::collections::HashMap;

use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, message::Delivery, options::*, types::{AMQPValue, FieldTable, ShortString}
};

use tracing::{debug, error, info, warn};

use crate::domain::errors::{consumer_error::ConsumerError, queue_error::QueueError};

pub struct RabbitMQConsumerImpl {
    pub channel: Channel,
    pub exchange_name: String,
    pub queue_name: String,
    pub max_retries: u32,
}

impl RabbitMQConsumerImpl {
    /// Crea una nueva instancia del consumidor de RabbitMQ
    pub async fn new(
        url: &str,
        exchange: &str,
        queue_name: &str,
        routing_key: &str,
        prefetch_count: u16,
        max_retries: u32,
    ) -> Result<Self, QueueError> {
        info!("Conectando a RabbitMQ: {}", url);

        // Establecer conexión
        let connection = Connection::connect(url, ConnectionProperties::default())
            .await
            .map_err(|e| {
                error!("Error conectando a RabbitMQ: {}", e);
                QueueError::ConnectionError(e.to_string())
            })?;

        info!("Conexión establecida a RabbitMQ");

        // Crear canal
        let channel = connection
            .create_channel()
            .await
            .map_err(|e| QueueError::ConnectionError(e.to_string()))?;

        Self::declarar_topologia(&channel, exchange, queue_name, routing_key).await?;

        Ok(Self {
            channel,
            exchange_name: exchange.to_string(),
            queue_name: queue_name.to_string(),
            max_retries, // Aquí puedes establecer un valor predeterminado o pasarlo como parámetro
        })
    }

    /// Declara el exchange, la cola de trabajo, su cola de descarte, y las liga.
    ///
    /// La declara el CONSUMIDOR y no el productor: el worker es quien sabe qué
    /// necesita recibir, y así el sistema se autoconfigura al arrancar en vez de
    /// depender de que alguien haya corrido un script. Medido antes de esto:
    /// `storage_tasks_exchange` no tenía NINGÚN binding, así que el orquestador
    /// publicaba al vacío y RabbitMQ descartaba cada mensaje en silencio — el
    /// archivo se subía, el webhook llegaba, y el worker nunca se enteraba.
    ///
    /// Todo es idempotente: declarar algo que ya existe con los mismos
    /// argumentos no hace nada.
    async fn declarar_topologia(
        channel: &Channel,
        exchange: &str,
        queue_name: &str,
        routing_key: &str,
    ) -> Result<(), QueueError> {
        if exchange.is_empty() || routing_key.is_empty() {
            warn!(
                exchange,
                routing_key,
                "Sin exchange o routing key: se consume la cola tal como esté, sin declarar nada"
            );
            return Ok(());
        }

        let dlx = format!("{exchange}.dlx");
        let dlq = format!("{queue_name}.dead");
        let durable = ExchangeDeclareOptions { durable: true, ..Default::default() };
        let cola = QueueDeclareOptions { durable: true, ..Default::default() };
        let err = |e: lapin::Error| QueueError::ConnectionError(e.to_string());

        // El de trabajo es `topic` porque el orquestador rutea por clave
        // (`media.document.upload`, `media.image.resize`…). El de descarte es
        // `fanout`: ahí no hay nada que rutear, todo lo que cae va al mismo lado.
        for (nombre, tipo) in [
            (exchange, lapin::ExchangeKind::Topic),
            (dlx.as_str(), lapin::ExchangeKind::Fanout),
        ] {
            channel
                .exchange_declare(nombre, tipo, durable, FieldTable::default())
                .await
                .map_err(err)?;
        }

        // La cola de descarte. Lo que agota sus reintentos termina acá en vez de
        // evaporarse: antes, al llegar al máximo, el worker logueaba "Publicando
        // mensaje de aborto" y hacía `nack(requeue=false)`, o sea que descartaba
        // el mensaje sin dejar rastro. El log afirmaba algo que no ocurría.
        channel.queue_declare(&dlq, cola, FieldTable::default()).await.map_err(err)?;
        channel
            .queue_bind(&dlq, &dlx, "", QueueBindOptions::default(), FieldTable::default())
            .await
            .map_err(err)?;

        // La cola de trabajo, apuntando su descarte al DLX.
        //
        // Los argumentos de una cola son INMUTABLES: si existiera ya sin
        // `x-dead-letter-exchange`, esta declaración falla con PRECONDITION_FAILED
        // y hay que borrarla a mano. Por eso se declara con su DLX desde el
        // principio y no "más adelante".
        let mut args = FieldTable::default();
        args.insert("x-dead-letter-exchange".into(), AMQPValue::LongString(dlx.clone().into()));
        channel.queue_declare(queue_name, cola, args).await.map_err(err)?;
        channel
            .queue_bind(queue_name, exchange, routing_key, QueueBindOptions::default(), FieldTable::default())
            .await
            .map_err(err)?;

        info!(
            exchange,
            queue = queue_name,
            routing_key,
            dead_letter = %dlq,
            "Topología de la cola declarada"
        );
        Ok(())
    }

    pub fn get_headers(delivery: &Delivery) -> HashMap<String, String> {
        delivery
            .properties
            .headers()
            .as_ref()
            .map(|h| {
                h.inner()
                    .iter()
                    .filter_map(|(k, v)| match v {
                        AMQPValue::LongString(s) => Some((k.to_string(), s.to_string())),
                        AMQPValue::ShortString(s) => Some((k.to_string(), s.to_string())),
                        AMQPValue::LongInt(b) => Some((k.to_string(), b.to_string())),
                        AMQPValue::Boolean(b) => Some((k.to_string(), b.to_string())),
                        _ => {
                            warn!("Tipo de header no soportado - key: {}, value: {:?}", k, v);
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn should_requeue(&self, headers: HashMap<String, String>) -> bool {
        let retry_count = headers
            .get("retry_count")
            .or_else(|| headers.get("retryCount"))
            .and_then(|value| value.trim().parse::<u32>().ok());

        match retry_count {
            Some(count) if count >= self.max_retries => {
                warn!(
                    "Mensaje alcanzó el máximo de reintentos ({}). No se reencolará.",
                    count
                );
                false
            }
            _ => true,
        }
    }

    pub async fn publish_retry_message(
        &self,
        message: Vec<u8>,
        headers: HashMap<String, String>,
    ) -> Result<u32, ConsumerError> {
        let target_routing_key = headers
            .get("routing_key")
            .cloned()
            .unwrap_or_else(|| "#".to_string());

        let new_retry_count = headers
            .get("retry_count")
            .or_else(|| headers.get("retryCount"))
            .and_then(|value| value.trim().parse::<u32>().ok())
            .unwrap_or(0) + 1;

        let mut amqp_headers = FieldTable::default();

        for (k, v) in &headers {
            if k != "retry_count" && k != "retryCount" {
                amqp_headers.insert(
                    ShortString::from(k.as_str()),
                    AMQPValue::LongString(v.clone().into()),
                );
            }
        }

        amqp_headers.insert(
            ShortString::from("retry_count"),
            AMQPValue::LongString(new_retry_count.to_string().into()),
        );

        let properties = BasicProperties::default().with_headers(amqp_headers);
        self.channel
            .basic_publish(
                &self.exchange_name,
                &target_routing_key,
                BasicPublishOptions::default(),
                &message,
                properties,
            )
            .await
            .map_err(|e| {
                ConsumerError::NackError(format!("Error publicando mensaje de reintento: {}", e))
            })?
            .await
            .map_err(|e| {
                ConsumerError::NackError(format!("Error confirmando mensaje de reintento: {}", e))
            })?;

        debug!(
            "Mensaje reencolado con retry_count={} para routing_key={}",
            new_retry_count, target_routing_key
        );

        Ok(new_retry_count)
    }
}
