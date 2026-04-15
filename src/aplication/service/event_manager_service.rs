use std::{collections::HashMap, sync::Arc};

use crate::domain::{
    errors::handler_error::HandlerError,
    models::{media_status_enum::MediaStatus::Uploaded, message_event_model::PublishPayload},
    ports::outbound::{
        object_db_repository::IObjectDBRepository,
        object_storage_repository::IObjectStorageRepository,
    },
};
use tracing::info;
pub struct EventManagerService {
    object_storage: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
    object_repository: Arc<dyn IObjectDBRepository>,
    // object_cache_repository: Arc<dyn IObjectCacheRepository>,
}

impl EventManagerService {
    pub fn new(
        object_storage: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
        object_repository: Arc<dyn IObjectDBRepository>,
        // object_cache_repository: Arc<dyn IObjectCacheRepository>,
    ) -> Self {
        Self {
            object_storage,
            object_repository,
            // object_cache_repository,
        }
    }

    pub async fn handle_image_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        self.object_repository
            .update_state(&_payload.event.asset_id, Uploaded)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        self.download_object_temp("", &_payload.event.storage_key).await;
        Ok(())
    }

    pub async fn handle_video_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje de video con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        self.object_repository
            .update_state(&_payload.event.asset_id, Uploaded)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    pub async fn handle_other_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje de otro tipo con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        self.object_repository
            .update_state(&_payload.event.asset_id, Uploaded)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    async fn download_object_temp(&self, _bucket: &str, _key: &str) -> Vec<String> {
        for service_storage in self.object_storage.values() {
            match service_storage.download_file(_bucket, _key).await {
                Ok(data) => {
                    info!(
                        "Archivo descargado exitosamente, tamaño: {} bytes",
                        data.len()
                    );
                    // Aquí podrías procesar los datos descargados según tus necesidades
                }
                Err(e) => {
                    info!("Error al descargar el archivo: {}", e);
                }
            }
        }
        vec![]
    }
}
