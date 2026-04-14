use std::collections::HashMap;

use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind, message::Delivery, options::*, types::{AMQPValue, FieldTable, ShortString}
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

        // Declarar el exchange
        channel
            .exchange_declare(
                &exchange,
                ExchangeKind::Topic, // Tipos existentes Direct, Fanout, Headers y Topic
                ExchangeDeclareOptions {
                    durable: true,
                    auto_delete: false,
                    internal: false,
                    nowait: false,
                    passive: false,
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConnectionError(e.to_string()))?;

        info!("Exchange declarado: {}", exchange);

        // Declarar la cola (asegura que existe)
        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConnectionError(e.to_string()))?;

        info!("Cola declarada: {}", queue_name);

        // Vincular la cola con el exchange
        channel
            .queue_bind(
                &queue_name,
                &exchange,
                "#", // Routing key pattern (# = todos los mensajes para exchange tipo Topic)
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| QueueError::ConnectionError(e.to_string()))?;

        info!("Cola vinculada al exchange: {} -> {}", exchange, queue_name);

        // Configurar prefetch
        channel
            .basic_qos(prefetch_count, BasicQosOptions::default())
            .await
            .map_err(|e| QueueError::ConnectionError(e.to_string()))?;

        info!(
            "Canal RabbitMQ configurado - Cola: {}, Prefetch: {}",
            queue_name, prefetch_count
        );

        Ok(Self {
            channel,
            exchange_name: exchange.to_string(),
            queue_name: queue_name.to_string(),
            max_retries, // Aquí puedes establecer un valor predeterminado o pasarlo como parámetro
        })
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
