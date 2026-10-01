//! Lectura del timbre electrónico: decodificación, parseo y verificación.
//!
//! Los fixtures son GENERADOS (ver `tests/fixtures/generar.py`), no facturas
//! reales: se versionan, y una factura real lleva datos de terceros. Además un
//! TED real no se puede anonimizar —el FRMT firma el DD completo— así que
//! cambiarle un campo para publicarlo rompería justo lo que hay que probar.
//!
//! El contraste contra facturas reales se hace con `lab-timbre`, a mano.
//!
//!     cargo test --features timbre

#![cfg(feature = "timbre")]

use worker_storage_processor::aplication::service::timbre_manager_service::TimbreManagerService;
use worker_storage_processor::domain::models::ted_model::Verificacion;

fn fixture(nombre: &str) -> image::DynamicImage {
    let ruta = format!("{}/tests/fixtures/{nombre}.png", env!("CARGO_MANIFEST_DIR"));
    image::open(&ruta).unwrap_or_else(|e| panic!("no se pudo abrir {ruta}: {e}"))
}

fn ted_original(nombre: &str) -> Vec<u8> {
    let ruta = format!("{}/tests/fixtures/{nombre}.ted.txt", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&ruta).unwrap_or_else(|e| panic!("no se pudo leer {ruta}: {e}"))
}

#[test]
fn lee_los_campos_del_timbre() {
    let lectura = TimbreManagerService::new()
        .leer(&fixture("valido"))
        .expect("el timbre tiene que decodificar");

    assert_eq!(lectura.ted.folio, 128);
    assert_eq!(lectura.ted.tipo_dte, 33);
    assert_eq!(lectura.ted.monto_total, 1_250_500);
    assert_eq!(lectura.ted.rut_receptor, "77777777-7");
    assert_eq!(lectura.ted.rut_emisor, "76543210-K");
    assert_eq!(lectura.ted.fecha_emision, "2026-06-14");
}

/// El caso que rompe si el decodificador interpreta UTF-8 en vez de entregar
/// los bytes: el TED viaja en ISO-8859-1.
#[test]
fn preserva_los_acentos_y_la_enie() {
    let lectura = TimbreManagerService::new().leer(&fixture("valido")).unwrap();
    assert_eq!(lectura.ted.razon_social_receptor, "PEÑALOLÉN DISTRIBUCIÓN LTDA");
    assert_eq!(
        lectura.ted.primer_item.as_deref(),
        Some("Señalización y montaje, instalación básica")
    );
}

/// Decodificar tiene que devolver EXACTAMENTE los bytes que se codificaron. Si
/// se pierde un solo byte alto, la firma deja de verificar.
#[test]
fn decodifica_byte_a_byte() {
    let ted = TimbreManagerService::new()
        .decodificar(&fixture("valido"))
        .expect("tiene que decodificar");
    let bytes: Vec<u8> = ted.chars().map(|c| c as u8).collect();
    let esperado = ted_original("valido");
    assert_eq!(bytes.len(), esperado.len(), "largo distinto");
    assert_eq!(bytes, esperado, "los bytes no coinciden con el original");
    assert!(
        esperado.iter().any(|&b| b > 0x7F),
        "el fixture tiene que contener bytes altos, si no la prueba no prueba nada"
    );
}

#[test]
fn la_firma_verifica() {
    let lectura = TimbreManagerService::new().leer(&fixture("valido")).unwrap();
    assert_eq!(lectura.verificacion, Verificacion::Verificado);
}

/// El control que le da sentido al anterior: una verificación que nunca falla
/// no prueba nada.
#[test]
fn detecta_el_monto_adulterado() {
    let lectura = TimbreManagerService::new().leer(&fixture("adulterado")).unwrap();
    assert_eq!(lectura.ted.monto_total, 9_999_999, "el timbre dice el monto alterado");
    assert_eq!(
        lectura.verificacion,
        Verificacion::FirmaInvalida,
        "cambiar el DD después de firmar TIENE que invalidar la firma"
    );
}

/// El DD se firma tal como lo armó el facturador. En las 4 facturas reales
/// medidas viaja **ya compacto** y verifica tal cual; pero nada obliga a eso,
/// así que el verificador tiene que aguantar las dos formas. Compactar siempre
/// rompería el caso de un DD con whitespace firmado tal cual.
#[test]
fn verifica_el_dd_venga_compacto_o_con_whitespace() {
    let servicio = TimbreManagerService::new();

    let pretty = servicio.decodificar(&fixture("valido")).unwrap();
    assert!(
        pretty.contains(">\r\n<") || pretty.contains(">\n<"),
        "el fixture `valido` tiene que traer whitespace entre tags, si no este \
         test no prueba el camino de compactación"
    );
    assert_eq!(
        servicio.leer(&fixture("valido")).unwrap().verificacion,
        Verificacion::Verificado
    );

    let compacto = servicio.decodificar(&fixture("compacto")).unwrap();
    let dd = &compacto[compacto.find("<DD>").unwrap()..compacto.find("</DD>").unwrap()];
    assert!(
        !dd.contains("> <") && !dd.contains(">\n<") && !dd.contains(">\r\n<"),
        "el fixture `compacto` tiene que venir sin whitespace, como las reales"
    );
    assert_eq!(
        servicio.leer(&fixture("compacto")).unwrap().verificacion,
        Verificacion::Verificado
    );
}

/// Solo 33, 34, 43 y 46 son cedibles: una boleta o una guía no se pueden ceder
/// a un factoring, y hoy nada lo impide porque el OCR ni intenta sacar el tipo.
#[test]
fn distingue_lo_que_no_es_cedible() {
    let servicio = TimbreManagerService::new();
    assert!(servicio.leer(&fixture("valido")).unwrap().ted.es_cedible());

    let boleta = servicio.leer(&fixture("no_cedible")).unwrap();
    assert_eq!(boleta.ted.tipo_dte, 39);
    assert!(!boleta.ted.es_cedible());
    assert_eq!(boleta.verificacion, Verificacion::Verificado,
               "el timbre es válido: lo que no sirve es el TIPO de documento");
}

/// Un respaldo sin timbre no es un error: hay documentos que legítimamente no
/// lo llevan (guías, cotizaciones, órdenes de compra). Ahí manda el OCR.
#[test]
fn una_imagen_sin_timbre_no_es_error() {
    let vacia = image::DynamicImage::ImageLuma8(image::ImageBuffer::from_pixel(
        800,
        400,
        image::Luma([255u8]),
    ));
    assert!(TimbreManagerService::new().leer(&vacia).is_none());
}
