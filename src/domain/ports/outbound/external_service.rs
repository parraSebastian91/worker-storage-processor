use async_trait::async_trait;

use crate::domain::models::factura_data_model::InvoiceData;

#[async_trait]
pub trait IExternalService {
    async fn notify_object_processed(
        &self,
        factura: InvoiceData,
        category: &str,
        status: &str,
        correlation_id: &str,
        owner_uuid: &str,
    ) -> String;
}
