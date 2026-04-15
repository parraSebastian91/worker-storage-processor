use std::{collections::HashMap, sync::Arc};

use crate::{
    aplication::service::event_manager_service::EventManagerService,
    domain::{
        errors::handler_error::HandlerError,
        models::{
            media_status_enum::MediaStatus::Uploaded, message_event_model::PublishPayload,
            MEDIA_TYPE_IMAGE, MEDIA_TYPE_VIDEO,
        },
        ports::{
            inbound::event_manager_usecase::IEventManagerUseCase,
            outbound::{
                object_db_repository::IObjectDBRepository,
                object_storage_repository::IObjectStorageRepository,
            },
        },
    },
};
use async_trait::async_trait;
use tracing::info;

pub struct EventManagerUseCase {
    object_storaje: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
    object_repository: Arc<dyn IObjectDBRepository>,
    event_manager_service: Arc<EventManagerService>,
    // object_cache_repository: Arc<dyn IObjectCacheRepository>,
}

#[async_trait]
impl IEventManagerUseCase for EventManagerUseCase {
    fn new(
        object_storaje: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
        object_repository: Arc<dyn IObjectDBRepository>,
        event_manager_service: Arc<EventManagerService>,
        // object_cache_repository: Arc<dyn IObjectCacheRepository>,
    ) -> Self {
        Self {
            object_storaje,
            object_repository,
            event_manager_service,
            // object_cache_repository,
        }
    }

    async fn handle_message(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        let _ = self.object_storaje.len();
        let _ = Arc::strong_count(&self.object_repository);

        info!("Manejando mensaje con EventManagerUseCase...");
        info!("Payload recibido: {:?}", _payload);

        match _payload.event.media_type.as_str() {
            MEDIA_TYPE_IMAGE => {
                self.event_manager_service
                    .handle_image_process(_payload)
                    .await?
            }
            // Aquí podrías agregar más casos para otros tipos de medios, por ejemplo:
            // "video" => self.handle_video_process(_payload).await?,
            MEDIA_TYPE_VIDEO => {
                self.event_manager_service
                    .handle_video_process(_payload)
                    .await?
            }
            _ => {
                info!("Tipo de medio no soportado: {}", _payload.event.media_type);
                return Err(HandlerError::UnsupportedMediaType(
                    _payload.event.media_type,
                ));
            }
        }

        // Aquí iría la lógica para manejar el evento, por ejemplo:
        // 1. Validar el payload
        // 2. Procesar el evento (ej. guardar en base de datos, actualizar cache, etc.)
        // 3. Manejar errores específicos y retornar un HandlerError si algo falla

        Ok(())
    }
}
