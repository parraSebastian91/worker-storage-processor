use std::{collections::HashMap, sync::Arc, time::Instant};

use crate::{
    aplication::service::event_manager_service::EventManagerService,
    domain::{
        errors::handler_error::HandlerError,
        models::{
            media_status_enum::MediaStatus::{Processing, Ready},
            message_event_model::PublishPayload,
            MEDIA_TYPE_DOCUMENT, MEDIA_TYPE_IMAGE, MEDIA_TYPE_VIDEO, STATE_PROCESS_READY,
        },
        ports::{
            inbound::event_manager_usecase::IEventManagerUseCase,
            outbound::{
                external_service::IExternalService, object_db_repository::IObjectDBRepository,
                object_storage_repository::IObjectStorageRepository,
            },
        },
    },
};
use async_trait::async_trait;
use tracing::{info, warn};

pub struct EventManagerUseCase {
    object_storaje: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
    object_repository: Arc<dyn IObjectDBRepository>,
    event_manager_service: Arc<EventManagerService>,
    external_services: Arc<dyn IExternalService + Send + Sync>,
    // object_cache_repository: Arc<dyn IObjectCacheRepository>,
}

#[async_trait]
impl IEventManagerUseCase for EventManagerUseCase {
    fn new(
        object_storaje: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
        object_repository: Arc<dyn IObjectDBRepository>,
        event_manager_service: Arc<EventManagerService>,
        external_services: Arc<dyn IExternalService + Send + Sync>,
        // object_cache_repository: Arc<dyn IObjectCacheRepository>,
    ) -> Self {
        Self {
            object_storaje,
            object_repository,
            event_manager_service,
            external_services,
            // object_cache_repository,
        }
    }

    async fn handle_message(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        let started_at = Instant::now();
        let correlation_id = _payload.correlation_id.as_deref().unwrap_or("n/a");
        let _ = self.object_storaje.len();
        let _ = Arc::strong_count(&self.object_repository);

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            media_type = %_payload.event.media_type,
            owner_uuid = %_payload.event.owner_uuid,
            storage_key = %_payload.event.storage_key,
            "Inicio de manejo de mensaje"
        );

        self.object_repository
            .update_state(&_payload.event.asset_id, Processing)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            new_state = "Processing",
            "Estado actualizado en base de datos"
        );

        let process_started_at = Instant::now();
        let _result_process = match _payload.event.media_type.as_str() {
            MEDIA_TYPE_IMAGE => {
                self.event_manager_service
                    .handle_image_process(_payload.clone(), true)
                    .await?
            }
            // Aquí podrías agregar más casos para otros tipos de medios, por ejemplo:
            // "video" => self.handle_video_process(_payload).await?,
            MEDIA_TYPE_VIDEO => {
                self.event_manager_service
                    .handle_video_process(_payload.clone(), false)
                    .await?
            }
            MEDIA_TYPE_DOCUMENT => {
                let factura_procesada = self
                    .event_manager_service
                    .handle_document_dte_process(_payload.clone(), true)
                    .await?;
                self.external_services
                    .notify_object_processed(
                        factura_procesada,
                        &_payload.event.category_process,
                        STATE_PROCESS_READY,
                        correlation_id,
                        &_payload.event.owner_uuid,
                    )
                    .await;
            }
            _ => {
                warn!(
                    correlation_id = %correlation_id,
                    asset_id = %_payload.event.asset_id,
                    media_type = %_payload.event.media_type,
                    "Tipo de medio no soportado"
                );
                return Err(HandlerError::UnsupportedMediaType(
                    _payload.event.media_type,
                ));
            }
        };
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            elapsed_ms = process_started_at.elapsed().as_millis(),
            "Procesamiento principal finalizado"
        );

        // let final_path = format!("`profile-pictures/{}/{}/%s-%s.%s`", _payload.event.storage_key);

        self.event_manager_service
            .delete_object_temp("", &_payload.event.storage_key)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            storage_key = %_payload.event.storage_key,
            "Objeto temporal eliminado"
        );

        let storage_key_final = _payload
            .event
            .storage_key
            .split_once("/temp")
            .map(|(base_path, _)| base_path.to_string())
            .unwrap_or_else(|| _payload.event.storage_key.clone());

        self.object_repository
            .update_state_and_key_storage(&_payload.event.asset_id, &storage_key_final, Ready)
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            new_state = "Ready",
            final_storage_key = %storage_key_final,
            elapsed_ms = started_at.elapsed().as_millis(),
            "Mensaje procesado completamente"
        );

        Ok(())
    }
}
