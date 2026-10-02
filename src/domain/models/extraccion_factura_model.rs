use serde::{Deserialize, Serialize};

use super::ted_model::{LecturaTimbre, OrigenDatos, Verificacion};

/// Un dato de la factura junto con **de dónde salió**.
///
/// El origen viaja pegado al valor y no en una cabecera aparte porque una misma
/// extracción mezcla fuentes: el timbre puede dar folio, RUT y monto, y la fecha
/// de vencimiento no salir de ningún lado. Aguas arriba la decisión cambia según
/// el origen, así que perderlo convierte una prueba y una conjetura en la misma
/// cosa.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Campo {
    pub valor: String,
    pub origen: OrigenDatos,
}

impl Campo {
    pub fn nuevo(valor: impl Into<String>, origen: OrigenDatos) -> Self {
        Self { valor: valor.into(), origen }
    }
}

/// Un campo donde el timbre y la capa de texto dicen cosas distintas.
///
/// No es un detalle de diagnóstico: es **la señal de adulteración**. Un PDF que
/// lleva la imagen de una factura real —timbre válido incluido— y una capa de
/// texto superpuesta con otro monto se ve idéntico a simple vista y engaña a
/// cualquier extractor que lea sólo el texto. Se midió: el timbre decía 883.456
/// y el texto 88.354.000. Si las dos capas existen y no coinciden, hay que
/// decirlo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Discrepancia {
    pub campo: String,
    pub segun_timbre: String,
    pub segun_texto: String,
}

/// Lo que el worker logró extraer de un respaldo, con su procedencia.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExtraccionFactura {
    pub folio: Option<Campo>,
    pub rut_emisor: Option<Campo>,
    pub rut_deudor: Option<Campo>,
    pub razon_social_deudor: Option<Campo>,
    pub monto_total: Option<Campo>,
    pub fecha_emision: Option<Campo>,

    /// Sólo lo da el timbre: el texto no permite saber si el documento es de un
    /// tipo que la ley deja ceder.
    pub tipo_dte: Option<u16>,
    pub es_cedible: Option<bool>,
    pub verificacion: Option<Verificacion>,

    /// Campos donde las capas se contradicen. Vacío es lo normal.
    pub discrepancias: Vec<Discrepancia>,
}

impl ExtraccionFactura {
    /// El origen menos confiable de los que aportaron algo. Es el que manda para
    /// decidir qué se le muestra a la ejecutiva: una factura cuyo monto salió
    /// del OCR no vale lo mismo que una cuyo monto salió del timbre, aunque el
    /// resto de los campos sí vengan firmados.
    pub fn origen_dominante(&self) -> Option<OrigenDatos> {
        [
            &self.folio,
            &self.rut_emisor,
            &self.rut_deudor,
            &self.razon_social_deudor,
            &self.monto_total,
            &self.fecha_emision,
        ]
        .iter()
        .filter_map(|c| c.as_ref().map(|c| c.origen))
        .max_by_key(|o| match o {
            OrigenDatos::TimbreVerificado => 0,
            OrigenDatos::TimbreSinVerificar => 1,
            OrigenDatos::CapaDeTexto => 2,
            OrigenDatos::Ocr => 3,
        })
    }

    /// Hay timbre con firma verificada **y** ninguna capa lo contradice.
    pub fn es_confiable(&self) -> bool {
        self.verificacion == Some(Verificacion::Verificado) && self.discrepancias.is_empty()
    }
}

impl From<&LecturaTimbre> for ExtraccionFactura {
    /// El timbre llena casi todo de una vez, y con un solo valor por campo — no
    /// candidatos. Lo único que no trae es la **fecha de vencimiento**: el TED
    /// certifica qué documento es y por cuánto, no cuándo se paga. Esa la
    /// declara el cedente (decisión registrada en `CLAUDE.md`, 2026-09-30).
    fn from(l: &LecturaTimbre) -> Self {
        let o = l.origen();
        let t = &l.ted;
        Self {
            folio: Some(Campo::nuevo(t.folio.to_string(), o)),
            rut_emisor: Some(Campo::nuevo(t.rut_emisor.clone(), o)),
            rut_deudor: Some(Campo::nuevo(t.rut_receptor.clone(), o)),
            razon_social_deudor: Some(Campo::nuevo(t.razon_social_receptor.clone(), o)),
            monto_total: Some(Campo::nuevo(t.monto_total.to_string(), o)),
            fecha_emision: Some(Campo::nuevo(t.fecha_emision.clone(), o)),
            tipo_dte: Some(t.tipo_dte),
            es_cedible: Some(t.es_cedible()),
            verificacion: Some(l.verificacion),
            discrepancias: Vec::new(),
        }
    }
}
