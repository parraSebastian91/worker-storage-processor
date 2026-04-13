use crate::domain::{
    errors::handler_error::HandlerError, models::message_event_model::PublishPayload,
};
use async_trait::async_trait;
/// Port para repositorio de metadatos de documentos
#[async_trait]
pub trait IEventManagerUseCase: Send + Sync {
    fn new(
        object_storaje: std::collections::HashMap<String, std::sync::Arc<dyn crate::domain::ports::outbound::object_storage_repository::IObjectStorageRepository + Send + Sync>>,
        object_repository: std::sync::Arc<
            dyn crate::domain::ports::outbound::object_db_repository::IObjectDBRepository,
        >,
        // object_cache_repository: std::sync::Arc<dyn crate::domain::ports::outbound::object_cache_repository::IObjectCacheRepository>,
    ) -> Self
    where
        Self: Sized;
    async fn handle_message(&self, payload: PublishPayload) -> Result<(), HandlerError>;
}
