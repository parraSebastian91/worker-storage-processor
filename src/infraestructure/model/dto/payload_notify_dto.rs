use serde::{Deserialize, Serialize};

use crate::domain::models::factura_data_model::InvoiceData;



#[derive(Debug, Clone, Serialize, Deserialize)]                
pub struct PayloadNotifyDTO {
    pub resource_type: String,
    pub resource_id: String,
    pub category: String,
    pub status: String,
    pub timestamp: String,
    pub app: String,
    pub correlation_id: String,
    pub owner_uuid: String,
    pub gestor: String,
    pub payload: InvoiceData,
    pub asset_id: String,
}                