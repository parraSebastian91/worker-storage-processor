use serde::{Deserialize, Serialize};

/// Campos del `<DD>` del Timbre Electrónico (TED) de un DTE chileno.
///
/// Un solo valor por campo, y exacto: lo puso el facturador del cedente y está
/// cubierto por la firma `<FRMT>`. Es lo contrario de lo que daba el OCR, que
/// eran listas de candidatos y había que elegir.
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
    /// Igual que `Verificado`, pero hubo que reparar un carácter que el símbolo
    /// impreso trae corrupto.
    ///
    /// No es una verificación más débil: la firma cuadra, y eso es prueba
    /// criptográfica de que el texto reparado es exactamente el que se firmó —
    /// no se puede "acertar" un SHA1 por casualidad. Se distingue de
    /// `Verificado` porque señala un **defecto del facturador**: su encoder
    /// metió un carácter no representable en el modo texto de PDF417 en vez de
    /// cambiar a modo byte, así que el timbre impreso no verifica tal cual para
    /// nadie. Vale reportarlo.
    VerificadoConReparacion,
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
    /// Regex sobre la capa de texto del PDF. Exacto en el sentido de que no hay
    /// ruido de lectura —son los bytes que puso el facturador—, pero **no está
    /// firmado**: superponerle una capa de texto a la imagen de una factura real
    /// es trivial y se midió que engaña a cualquier extractor que la crea
    /// (`docs/general/findings/cotizadores-externos/` §4.2). Sirve para
    /// prellenar y para contrastar; no para decidir.
    CapaDeTexto,
    /// Regex sobre Tesseract: candidatos, no certezas.
    Ocr,
}

impl OrigenDatos {
    /// Orden de confianza. Sólo el timbre verificado es prueba: falsificarlo
    /// requiere un CAF válido del SII. Lo demás es declaración.
    pub fn es_prueba(&self) -> bool {
        matches!(self, OrigenDatos::TimbreVerificado)
    }
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
            Verificacion::Verificado | Verificacion::VerificadoConReparacion => {
                OrigenDatos::TimbreVerificado
            }
            _ => OrigenDatos::TimbreSinVerificar,
        }
    }
}
