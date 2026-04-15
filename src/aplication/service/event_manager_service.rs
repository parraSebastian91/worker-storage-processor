use std::{collections::HashMap, sync::Arc, time::Instant};

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
use regex::Regex;
use tracing::{error, info, warn};
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
        let started_at = Instant::now();
        let correlation_id = _payload.correlation_id.as_deref().unwrap_or("n/a");
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            media_type = %_payload.event.media_type,
            storage_key = %_payload.event.storage_key,
            recipe_name = %_payload.recipe.name,
            "Inicio de procesamiento de imagen"
        );

        let download_started_at = Instant::now();
        let object = self
            .download_object_temp("", &_payload.event.storage_key)
            .await;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            object_bytes = object.len(),
            elapsed_ms = download_started_at.elapsed().as_millis(),
            "Descarga de objeto temporal finalizada"
        );

        let process_started_at = Instant::now();
        let process_result = self
            .image_process_service
            .process(&object, &_payload.recipe)
            .map_err(|e| HandlerError::ProcessingError(e.to_string()))?;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            variants_generated = process_result.len(),
            elapsed_ms = process_started_at.elapsed().as_millis(),
            "Transformacion de imagen finalizada"
        );

        for media in process_result {
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                format = %media.format,
                size = %media.size,
                width = media.width,
                height = media.height,
                bytes = media.bytes.len(),
                "Variante procesada"
            );
            let key_object = format!(
                "public/profile-pictures/{}/{}/{}-{}.{}",
                _payload.event.owner_uuid,
                _payload.event.category_process,
                _payload.event.name_file,
                media.size,
                media.format
            );

            self.upload_object_final("", &key_object as &str, media.bytes.clone())
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                object_key = %key_object,
                "Variante subida a storage"
            );

            let metadata = VariantMetadataModel {
                format: media.format.clone(),
                size: media.size,
                width: media.width,
                height: media.height,
                headers: "Cache-Control: public, max-age=31536000".to_string()
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
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                object_key = %key_object,
                "Metadata de variante persistida"
            );
        }

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            elapsed_ms = started_at.elapsed().as_millis(),
            "Procesamiento de imagen completado"
        );

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
        let total_providers = self.object_storage.len();
        for (index, service_storage) in self.object_storage.values().enumerate() {
            info!(
                provider_attempt = index + 1,
                total_providers,
                bucket = _bucket,
                object_key = _key,
                "Intentando descargar objeto temporal"
            );
            match service_storage.download_file(_bucket, _key).await {
                Ok(data) => {
                    info!(
                        bucket = _bucket,
                        object_key = _key,
                        bytes = data.len(),
                        provider_attempt = index + 1,
                        "Archivo descargado exitosamente"
                    );
                    // Aquí podrías procesar los datos descargados según tus necesidades
                    return data;
                }
                Err(e) => {
                    warn!(
                        bucket = _bucket,
                        object_key = _key,
                        provider_attempt = index + 1,
                        error = %e,
                        "Fallo la descarga en proveedor actual"
                    );
                    continue;
                }
            }
        }
        error!(
            bucket = _bucket,
            object_key = _key,
            "No se pudo descargar el objeto temporal en ningun proveedor"
        );
        vec![]
    }

    async fn upload_object_final(
        &self,
        _bucket: &str,
        _key: &str,
        _data: Vec<u8>,
    ) -> Result<(), HandlerError> {
        let spaces_regex = Regex::new(r"\s+")
            .map_err(|e| HandlerError::RepositoryError(format!("Regex inválida: {}", e)))?;
        let key_no_spaces = spaces_regex.replace_all(_key.trim(), "_");
        let total_providers = self.object_storage.len();
        for (index, service_storage) in self.object_storage.values().enumerate() {
            info!(
                provider_attempt = index + 1,
                total_providers,
                bucket = _bucket,
                object_key = %key_no_spaces,
                bytes = _data.len(),
                "Intentando subir variante final"
            );
            match service_storage
                .upload_file(_bucket, &key_no_spaces, _data.clone())
                .await
            {
                Ok(_) => {
                    info!(
                        bucket = _bucket,
                        object_key = %key_no_spaces,
                        provider_attempt = index + 1,
                        "Archivo subido exitosamente"
                    );
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        bucket = _bucket,
                        object_key = %key_no_spaces,
                        provider_attempt = index + 1,
                        error = %e,
                        "Fallo la subida en proveedor actual"
                    );
                    continue;
                }
            }
        }
        error!(
            bucket = _bucket,
            object_key = %key_no_spaces,
            "No se pudo subir el archivo a ningun proveedor"
        );
        Err(HandlerError::RepositoryError(
            "No se pudo subir el archivo a ningún servicio de almacenamiento".to_string(),
        ))
    }

    pub async fn delete_object_temp(&self, _bucket: &str, _key: &str) -> Result<(), HandlerError> {
        let total_providers = self.object_storage.len();
        for (index, service_storage) in self.object_storage.values().enumerate() {
            info!(
                provider_attempt = index + 1,
                total_providers,
                bucket = _bucket,
                object_key = _key,
                "Intentando eliminar objeto temporal"
            );
            match service_storage.delete_file(_bucket, _key).await {
                Ok(_) => {
                    info!(
                        bucket = _bucket,
                        object_key = _key,
                        provider_attempt = index + 1,
                        "Archivo eliminado exitosamente"
                    );
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        bucket = _bucket,
                        object_key = _key,
                        provider_attempt = index + 1,
                        error = %e,
                        "Fallo la eliminacion en proveedor actual"
                    );
                    continue;
                }
            }
        }
        error!(
            bucket = _bucket,
            object_key = _key,
            "No se pudo eliminar el archivo temporal en ningun proveedor"
        );
        Err(HandlerError::RepositoryError(
            "No se pudo eliminar el archivo de ningún servicio de almacenamiento".to_string(),
        ))
    }
}
