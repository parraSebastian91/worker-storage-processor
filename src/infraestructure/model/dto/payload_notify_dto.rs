use serde::{Deserialize, Serialize};

use crate::domain::models::extraccion_factura_model::ExtraccionFactura;

/// Lo que el worker manda cuando termina de procesar un documento.
///
/// `payload` dejó de ser `InvoiceData` —listas de candidatos del OCR— y pasó a
/// ser la extracción con la procedencia de cada campo. El cambio importa aguas
/// arriba: con candidatos, quien recibía tenía que elegir cuál; con esto recibe
/// un valor por campo y de dónde salió, así que puede decidir si lo muestra como
/// dato o como sugerencia a confirmar.
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
    /// Vacío cuando el documento no dio nada por ninguna capa.
    pub payload: Option<ExtraccionFactura>,
    pub asset_id: String,
}
