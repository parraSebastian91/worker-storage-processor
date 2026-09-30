use serde::{Deserialize, Serialize};

/// Campos del `<DD>` del Timbre Electrónico (TED) de un DTE chileno.
///
/// A diferencia de `InvoiceData` —que son *candidatos* del OCR— acá hay un solo
/// valor por campo y es exacto: lo puso el facturador del cedente y está
/// cubierto por la firma `<FRMT>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ted {
    /// `RE` — RUT del emisor
    pub rut_emisor: String,
    /// `TD` — tipo de DTE. Solo 33, 34, 43 y 46 son cedibles.
    pub tipo_dte: u16,
    /// `F` — folio (el "número de factura")
    pub folio: u64,
    /// `FE` — fecha de emisión, `YYYY-MM-DD`
    pub fecha_emision: String,
    /// `RR` — RUT del receptor (el deudor de la operación de factoring)
    pub rut_receptor: String,
    /// `RSR` — razón social del receptor
    pub razon_social_receptor: String,
    /// `MNT` — monto total
    pub monto_total: i64,
    /// `IT1` — descripción del primer ítem
    pub primer_item: Option<String>,
}

impl Ted {
    /// Tipos de DTE que la ley permite ceder (factura electrónica y sus
    /// variantes). Una boleta o una guía de despacho no se pueden ceder.
    pub const TIPOS_CEDIBLES: [u16; 4] = [33, 34, 43, 46];

    pub fn es_cedible(&self) -> bool {
        Self::TIPOS_CEDIBLES.contains(&self.tipo_dte)
    }
}

/// Resultado de contrastar el `<FRMT>` contra la clave pública del `<CAF>`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Verificacion {
    /// La firma corresponde al DD: el documento no fue alterado después de
    /// timbrarse, y lo timbró quien tiene la privada de ese CAF.
    Verificado,
    /// El timbre se leyó pero la firma no cuadra: DD adulterado, o un CAF que
    /// no corresponde.
    FirmaInvalida,
    /// El CAF no trae `<RSAPK>` o no se pudo armar la clave con él.
    SinClavePublica,
    /// El TED decodificó pero le falta `<DD>` o `<FRMT>`.
    TedIncompleto,
}

/// De dónde salieron los datos de la factura. Aguas arriba la decisión cambia
/// según el origen, así que viaja junto con los valores.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrigenDatos {
    /// Del timbre, con la firma verificada. Es la fuente de verdad.
    TimbreVerificado,
    /// Del timbre, pero la firma no verificó. Los valores sirven para comparar,
    /// no para confiar.
    TimbreSinVerificar,
    /// Regex sobre Tesseract: candidatos, no certezas.
    Ocr,
}

/// Lo que devuelve la lectura de un timbre.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LecturaTimbre {
    pub ted: Ted,
    pub verificacion: Verificacion,
}

impl LecturaTimbre {
    pub fn origen(&self) -> OrigenDatos {
        match self.verificacion {
            Verificacion::Verificado => OrigenDatos::TimbreVerificado,
            _ => OrigenDatos::TimbreSinVerificar,
        }
    }
}
