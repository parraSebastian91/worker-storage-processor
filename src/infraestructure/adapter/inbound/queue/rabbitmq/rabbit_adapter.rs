use crate::{
    domain::{
        errors::consumer_error::ConsumerError,
        models::message_event_model::PublishPayload,
        ports::inbound::queue_handler::{IQueueConsumer, QueueHandler},
    },
    infraestructure::adapter::inbound::queue::rabbitmq::queue_client::RabbitMQConsumerImpl,
};
use async_trait::async_trait;
use futures_lite::stream::StreamExt;
use lapin::{
    options::*,
    types::{FieldTable},
};
use tracing::{debug, error, info};

#[async_trait]
impl IQueueConsumer for RabbitMQConsumerImpl {
    async fn consume(&self, handler: QueueHandler) -> Result<(), ConsumerError> {
        info!(
            "Iniciando consumo de mensajes de la cola: {}",
            self.queue_name
        );

        let mut consumer = self
            .channel
            .basic_consume(
                &self.queue_name,
                "worker_storage_consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| {
                error!("Error iniciando consumidor: {}", e);
                ConsumerError::ConsumeError(e.to_string())
            })?;

        info!("Consumidor iniciado. Esperando mensajes...");

        while let Some(delivery_result) = consumer.next().await {
            match delivery_result {
                Ok(delivery) => {
                    let delivery_tag = delivery.delivery_tag;
                    debug!("Mensaje recibido - delivery_tag: {}", delivery_tag);

                    // Deserializar el mensaje
                    let payload = match serde_json::from_slice::<PublishPayload>(&delivery.data) {
                        Ok(p) => p,
                        Err(e) => {
                            error!("Error deserializando mensaje: {}", e);
                            // Nack con requeue para intentar procesar nuevamente
                            if let Err(nack_err) = self.nack(delivery_tag, true).await {
                                error!("Error enviando nack: {}", nack_err);
                            }
                            continue; // Saltar al siguiente mensaje
                        }
                    };
                    debug!("Payload deserializado: {:?}", payload);

                    match handler(payload).await {
                        Ok(_) => {
                            debug!(
                                "Mensaje procesado exitosamente - delivery_tag: {}",
                                delivery_tag
                            );
                            if let Err(ack_err) = self.ack(delivery_tag).await {
                                error!("Error enviando ack: {}", ack_err);
                            }
                        }
                        Err(e) => {
                            error!("Error procesando mensaje: {}", e);
                            // Nack con requeue para intentar procesar nuevamente
                            if let Err(nack_err) = self.nack(delivery_tag, true).await {
                                error!("Error enviando nack: {}", nack_err);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Error al consumir mensaje: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn ack(&self, delivery_tag: u64) -> Result<(), ConsumerError> {
        debug!("ACK mensaje - delivery_tag: {}", delivery_tag);

        self.channel
            .basic_ack(delivery_tag, BasicAckOptions::default())
            .await
            .map_err(|e| {
                error!("Error haciendo ACK: {}", e);
                ConsumerError::AckError(e.to_string())
            })
    }

    async fn nack(&self, delivery_tag: u64, requeue: bool) -> Result<(), ConsumerError> {
        debug!(
            "NACK mensaje - delivery_tag: {}, requeue: {}",
            delivery_tag, requeue
        );

        self.channel
            .basic_nack(
                delivery_tag,
                BasicNackOptions {
                    requeue,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| {
                error!("Error haciendo NACK: {}", e);
                ConsumerError::NackError(e.to_string())
            })
    }

    async fn close(&self) -> Result<(), ConsumerError> {
        info!("Cerrando canal de RabbitMQ");

        self.channel
            .close(200, "Normal shutdown")
            .await
            .map_err(|e| ConsumerError::ConnectionError(e.to_string()))
    }
}
