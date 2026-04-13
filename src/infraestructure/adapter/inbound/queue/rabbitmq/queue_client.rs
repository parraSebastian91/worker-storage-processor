use lapin::{
    options::*, types::FieldTable, Channel, Connection, ConnectionProperties, ExchangeKind,
};

use tracing::{error, info};

use crate::domain::errors::queue_error::QueueError;

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
}
