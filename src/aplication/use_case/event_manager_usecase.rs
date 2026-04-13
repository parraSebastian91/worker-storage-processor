use std::{collections::HashMap, sync::Arc};

use crate::domain::{
    errors::handler_error::HandlerError,
    models::message_event_model::PublishPayload,
    ports::{
        inbound::event_manager_usecase::IEventManagerUseCase,
        outbound::{
            object_db_repository::IObjectDBRepository,
            object_storage_repository::IObjectStorageRepository,
        },
    },
};
use tracing::info;
use async_trait::async_trait;

pub struct EventManagerUseCase {
    object_storaje: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
    object_repository: Arc<dyn IObjectDBRepository>,
    // object_cache_repository: Arc<dyn IObjectCacheRepository>,
}

#[async_trait]
impl IEventManagerUseCase for EventManagerUseCase {
    fn new(
        object_storaje: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
        object_repository: Arc<dyn IObjectDBRepository>,
        // object_cache_repository: Arc<dyn IObjectCacheRepository>,
    ) -> Self {
        Self {
            object_storaje,
            object_repository,
            // object_cache_repository,
        }
    }

    async fn handle_message(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        let _ = self.object_storaje.len();
        let _ = Arc::strong_count(&self.object_repository);

        info!("Manejando mensaje con EventManagerUseCase...");
        info!("Payload recibido: {:?}", _payload);

        // Aquí iría la lógica para manejar el evento, por ejemplo:
        // 1. Validar el payload
        // 2. Procesar el evento (ej. guardar en base de datos, actualizar cache, etc.)
        // 3. Manejar errores específicos y retornar un HandlerError si algo falla

        Ok(())
    }
}
