use std::{collections::HashMap, sync::Arc};

use crate::{
    aplication::service::image_manager_service::ImageManagerService,
    domain::{
        errors::handler_error::HandlerError,
        models::{
            media_status_enum::MediaStatus::Processing,
            message_event_model::{PublishPayload, VariantMetadataModel, VariantModel},
        },
        ports::outbound::{
            object_db_repository::IObjectDBRepository,
            object_storage_repository::IObjectStorageRepository,
        },
    },
};
use tracing::info;
pub struct EventManagerService {
    object_storage: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
    object_repository: Arc<dyn IObjectDBRepository>,
    image_process_service: Arc<ImageManagerService>,
    // object_cache_repository: Arc<dyn IObjectCacheRepository>,
}

impl EventManagerService {
    pub fn new(
        object_storage: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
        object_repository: Arc<dyn IObjectDBRepository>,
        // object_cache_repository: Arc<dyn IObjectCacheRepository>,
        image_process_service: Arc<ImageManagerService>,
    ) -> Self {
        Self {
            object_storage,
            object_repository,
            // object_cache_repository,
            image_process_service,
        }
    }

    pub async fn handle_image_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);

        let object = self
            .download_object_temp("", &_payload.event.storage_key)
            .await;

        let process_result = self
            .image_process_service
            .process(&object, &_payload.recipe)
            .map_err(|e| HandlerError::ProcessingError(e.to_string()))?;

        for media in process_result {
            info!(
                "Media procesada: format={}, size={}, width={}, height={}",
                media.format, media.size, media.width, media.height
            );
            let key_object = format!(
                "profile-pictures/{}/{}/{}-{}.{}",
                _payload.event.owner_uuid,
                _payload.event.category_process,
                _payload.event.name_file,
                media.size,
                media.format
            );

            self.upload_object_final("", &key_object as &str, media.bytes.clone())
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

            let metadata = VariantMetadataModel {
                format: media.format.clone(),
                size: media.size,
                width: media.width,
                height: media.height,
            };

            let media_variant = VariantModel {
                asset_id: _payload.event.asset_id.clone(),
                name: _payload.event.name_file.clone(),
                metadata: metadata,
                url_path: key_object.clone(),
            };

            self.object_repository
                .create_variant(media_variant.into())
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn handle_video_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje de video con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        self.object_repository
            .update_state(&_payload.event.asset_id, Processing)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    pub async fn handle_other_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje de otro tipo con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        self.object_repository
            .update_state(&_payload.event.asset_id, Processing)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    async fn download_object_temp(&self, _bucket: &str, _key: &str) -> Vec<u8> {
        for service_storage in self.object_storage.values() {
            match service_storage.download_file(_bucket, _key).await {
                Ok(data) => {
                    info!(
                        "Archivo descargado exitosamente, tamaño: {} bytes",
                        data.len()
                    );
                    // Aquí podrías procesar los datos descargados según tus necesidades
                    return data;
                }
                Err(e) => {
                    info!("Error al descargar el archivo: {}", e);
                    continue;
                }
            }
        }
        vec![]
    }

    async fn upload_object_final(
        &self,
        _bucket: &str,
        _key: &str,
        _data: Vec<u8>,
    ) -> Result<(), HandlerError> {
        for service_storage in self.object_storage.values() {
            match service_storage
                .upload_file(_bucket, _key, _data.clone())
                .await
            {
                Ok(_) => {
                    info!(
                        "Archivo subido exitosamente a {} con clave {}",
                        _bucket, _key
                    );
                    return Ok(());
                }
                Err(e) => {
                    info!("Error al subir el archivo: {}", e);
                    continue;
                }
            }
        }
        Err(HandlerError::RepositoryError(
            "No se pudo subir el archivo a ningún servicio de almacenamiento".to_string(),
        ))
    }
}
