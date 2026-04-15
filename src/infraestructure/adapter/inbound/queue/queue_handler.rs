use std::sync::Arc;

use crate::domain::{
    errors::queue_error::QueueError,
    models::message_event_model::PublishPayload,
    ports::inbound::{
        event_manager_usecase::IEventManagerUseCase,
        queue_handler::{IQueueConsumer, QueueHandler as QueueMessageHandler},
    },
};
use tracing::info;
pub struct QueueHandler {
    consumer: Arc<dyn IQueueConsumer>,
    process_service: Arc<dyn IEventManagerUseCase>,
}

impl QueueHandler {
    pub fn new(
        queue_consumer: Arc<dyn IQueueConsumer>,
        event_manager_use_case: Arc<dyn IEventManagerUseCase>,
    ) -> Self {
        Self {
            consumer: queue_consumer,
            process_service: event_manager_use_case,
        }
    }

    pub async fn run(&self) -> Result<(), QueueError> {
        info!("🚀 Iniciando worker de documentos...");

        let process_service_clone = Arc::clone(&self.process_service);

        // Crear el handler que delega al application service
        let handler: QueueMessageHandler = Arc::new(move |payload: PublishPayload| {
            let service = Arc::clone(&process_service_clone);
            Box::pin(async move { service.handle_message(payload).await })
        });

        self.consumer
            .consume(handler)
            .await
            .map_err(|e| QueueError::ConsumerError(e.to_string()))?;

        Ok(())
    }

    /// Detiene el worker de forma ordenada
    pub async fn shutdown(&self) -> Result<(), QueueError> {
        info!("Deteniendo worker...");

        self.consumer
            .close()
            .await
            .map_err(|e| QueueError::ConsumerError(e.to_string()))?;

        info!("Worker detenido");
        Ok(())
    }
}
