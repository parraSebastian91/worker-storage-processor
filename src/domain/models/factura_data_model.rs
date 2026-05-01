use serde::{Deserialize, Serialize};

/// Datos extraídos de una factura chilena DTE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceData {
    /// Números de factura encontrados (ej: [Nº 123456, Nº 789])
    pub numero_factura: Vec<String>,
    /// RUTs del deudor encontrados (ej: [12.345.678-9])
    pub rut_deudor: Vec<String>,
    /// Nombres del deudor encontrados (ej: [JUAN PÉREZ])
    pub nombre_deudor: Vec<String>,
    /// Montos totales encontrados (ej: [1.250.500])
    pub monto_total: Vec<String>,
    /// Texto completo del OCR por línea (normalizado)
    pub full_text: Vec<String>,
}
