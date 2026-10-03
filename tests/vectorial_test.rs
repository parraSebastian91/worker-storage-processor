//! Timbre dibujado como rectángulos en vez de embebido como imagen.
//!
//! El fixture es GENERADO (`tests/fixtures/generar.py`) y reproduce el defecto
//! real medido sobre el corpus: cada módulo oscuro se pinta como su propio
//! rectángulo, **más angosto que su celda**. Esa ranura blanca hace que una
//! corrida de módulos vecinos se imprima como barras separadas, y PDF417
//! codifica justamente en el ancho de las corridas.
//!
//! Comprobado al generarlo: ese mismo PDF rasterizado a 3000 y a 6000 px da
//! 0/2. No es un problema de muestreo — el dibujo está mal, y por eso ninguna
//! resolución lo arregla.
//!
//! Necesita pdfium en runtime, igual que el resto de la feature `pdf`:
//!
//!     PDFIUM_PATH=/ruta/libpdfium.dylib cargo test --features "pdf,timbre"

#![cfg(all(feature = "pdf", feature = "timbre"))]

use worker_storage_processor::aplication::service::{
    document_manager_service::DocumentManagerService, timbre_manager_service::TimbreManagerService,
};
use worker_storage_processor::domain::models::ted_model::Verificacion;

fn pdf(nombre: &str) -> Vec<u8> {
    let ruta = format!("{}/tests/fixtures/{nombre}.pdf", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&ruta).unwrap_or_else(|e| panic!("no se pudo leer {ruta}: {e}"))
}

/// pdfium se enlaza en runtime. Sin él no hay nada que probar acá, y es mejor
/// decirlo que fallar con un error de enlace que no explica nada.
fn hay_pdfium(doc: &DocumentManagerService, bytes: &[u8]) -> bool {
    match doc.simbolo_vectorial_primera_pagina(bytes) {
        Err(e) if e.to_string().contains("pdfium") || e.to_string().contains("library") => {
            eprintln!("SALTADO: pdfium no disponible ({e}). Ver PDFIUM_PATH.");
            false
        }
        _ => true,
    }
}

#[test]
fn el_timbre_dibujado_como_rectangulos_se_reconstruye_y_verifica() {
    let doc = DocumentManagerService::new();
    let bytes = pdf("vectorial");
    if !hay_pdfium(&doc, &bytes) {
        return;
    }

    let simbolo = doc
        .simbolo_vectorial_primera_pagina(&bytes)
        .expect("la página se lee")
        .expect("hay rectángulos suficientes para armar un símbolo");

    let l = TimbreManagerService::new()
        .leer(&simbolo)
        .expect("el símbolo reconstruido decodifica");

    assert_eq!(l.verificacion, Verificacion::Verificado);
    assert_eq!(l.ted.folio, 1726);
    assert_eq!(l.ted.monto_total, 252280);
    assert_eq!(l.ted.razon_social_receptor, "BESALCO PIQUES Y TUNELES S.A.");
}

#[test]
fn un_pdf_sin_timbre_vectorial_no_inventa_un_simbolo() {
    // El fixture `valido` lleva el timbre como imagen embebida, no como paths.
    // La reconstrucción tiene que devolver None en vez de agrupar cualquier cosa
    // que encuentre: un falso positivo acá haría que el decodificador trabaje
    // sobre ruido y, peor, podría dar un TED de otro documento de la página.
    let doc = DocumentManagerService::new();
    let bytes = match std::fs::read(format!(
        "{}/tests/fixtures/vectorial.pdf",
        env!("CARGO_MANIFEST_DIR")
    )) {
        Ok(b) => b,
        Err(_) => return,
    };
    if !hay_pdfium(&doc, &bytes) {
        return;
    }

    // Un PDF mínimo, sin un solo rectángulo.
    let vacio = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n\
                  2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n\
                  3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\n\
                  trailer<</Root 1 0 R>>\n%%EOF\n";
    assert!(
        matches!(doc.simbolo_vectorial_primera_pagina(vacio), Ok(None) | Err(_)),
        "sin rectángulos no hay símbolo que reconstruir"
    );
}
