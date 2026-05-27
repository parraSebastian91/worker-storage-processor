use crate::domain::{
    models::factura_data_model::InvoiceData, ports::outbound::external_service::IExternalService,
};
use crate::infraestructure::config::app_config::ConfiguracionGral;
use crate::infraestructure::model::dto::payload_notify_dto::PayloadNotifyDTO;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::{error, info};

pub struct ExternalServicesImpl {
    general_config: ConfiguracionGral,
    http_client: Client,
}

impl ExternalServicesImpl {
    pub fn new(general_config: ConfiguracionGral) -> Self {
        ExternalServicesImpl {
            general_config,
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl IExternalService for ExternalServicesImpl {
    async fn notify_object_processed(
        &self,
        factura: InvoiceData,
        category: &str,
        status: &str,
        correlation_id: &str,
        owner_uuid: &str,
        gestor: &str,
        asset_id: &str,
        resource_id: &str,
        resource_type: &str,
    ) -> String {
        if let Some(webhook_url) = &self.general_config.external_orchestrator_url {
            let payload = PayloadNotifyDTO {
                category: category.to_string(),
                status: status.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                app: self.general_config.app_name.clone(),
                correlation_id: correlation_id.to_string(),
                owner_uuid: owner_uuid.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                payload: factura.clone(),
                gestor: gestor.to_string(),
                asset_id: asset_id.to_string(),
            };

            match self
                .http_client
                .put(format!("{}/webhooks/notify", webhook_url))
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        info!(
                            "Webhook notificacion enviada exitosamente a: {}",
                            webhook_url
                        );
                        format!("Objeto procesado notificado en {}", webhook_url)
                    } else {
                        error!(
                            "Webhook retorno error: {} - {}",
                            response.status(),
                            webhook_url
                        );
                        format!("Error en webhook: {}", response.status())
                    }
                }
                Err(e) => {
                    error!("Error al notificar webhook: {}", e);
                    format!("Error al notificar webhook: {}", e)
                }
            }
        } else {
            info!("EXTERNAL_ORCHESTRATOR_URL no configurada, webhook no enviado");
            "Webhook URL no configurada".to_string()
        }
    }
}
