//! La capa de texto, probada sobre textos sintéticos.
//!
//! Sintéticos a propósito: las facturas reales no se commitean (el repo del
//! worker es público y traen RUT y razón social de terceros), pero sus *formas*
//! sí, y son lo que importa. Cada caso de acá reproduce una forma medida sobre
//! las 6 facturas reales de `fixtures/timbres/reales/`.

use worker_storage_processor::aplication::service::extraccion_factura_service::ExtraccionFacturaService;
use worker_storage_processor::domain::models::ted_model::OrigenDatos;

fn s() -> ExtraccionFacturaService {
    ExtraccionFacturaService::new()
}

/// Forma del recuadro del SII tal como lo imprimen la mayoría de los
/// facturadores: RUT del emisor arriba, bloque "SEÑOR(ES)" para el receptor.
const FACTURA: &str = "\
R.U.T.: 76.702.579-3 FACTURA ELECTRONICA N° 8782 \
S.I.I. - SANTIAGO CENTRO \
Fecha Emision: 30/09/2026 \
SEÑOR(ES): MEDITERRANEAN SHIPPING COMPANY R.U.T.: 96.707.720-8 \
Giro: TRANSPORTE MARITIMO \
MONTO NETO $ 742.400 I.V.A. 19% $ 141.056 IMPUESTO ADICIONAL $ 0 TOTAL $ 883.456";

#[test]
fn lee_los_campos_del_recuadro_del_sii() {
    let e = s().desde_texto(FACTURA);
    assert_eq!(e.folio.as_ref().unwrap().valor, "8782");
    assert_eq!(e.rut_emisor.as_ref().unwrap().valor, "76702579-3");
    assert_eq!(e.rut_deudor.as_ref().unwrap().valor, "96707720-8");
    assert_eq!(e.monto_total.as_ref().unwrap().valor, "883456");
    assert_eq!(e.fecha_emision.as_ref().unwrap().valor, "2026-09-30");
    assert_eq!(
        e.razon_social_deudor.as_ref().unwrap().valor,
        "MEDITERRANEAN SHIPPING COMPANY"
    );
}

#[test]
fn todo_lo_que_sale_del_texto_queda_marcado_como_texto() {
    // Importa tanto como los valores: aguas arriba, un monto que salió del texto
    // no puede tratarse igual que uno firmado por el timbre.
    let e = s().desde_texto(FACTURA);
    for c in [&e.folio, &e.rut_emisor, &e.rut_deudor, &e.monto_total] {
        assert_eq!(c.as_ref().unwrap().origen, OrigenDatos::CapaDeTexto);
    }
    assert!(!OrigenDatos::CapaDeTexto.es_prueba());
    assert_eq!(e.origen_dominante(), Some(OrigenDatos::CapaDeTexto));
}

#[test]
fn el_impuesto_adicional_en_cero_no_corre_el_total() {
    // El caso exacto que duplicaba el monto en el cotizador analizado: su patrón
    // de dinero exigía separador de miles, no podía matchear el "0", y se
    // llevaba el total para después sumarlo.
    let e = s().desde_texto(FACTURA);
    assert_eq!(e.monto_total.unwrap().valor, "883456", "el total es el total, no el doble");
}

#[test]
fn el_rut_del_deudor_sale_del_bloque_senores_y_no_del_orden() {
    // Un PDF que imprime el RUT del deudor ANTES que el del emisor rompe al que
    // se queda con "el primer RUT que coincida".
    let invertida = "\
        Cliente 96.707.720-8 en nuestros registros \
        R.U.T.: 76.702.579-3 FACTURA ELECTRONICA N° 8782 \
        SEÑOR(ES): NAVIERA DEL SUR SPA R.U.T.: 77.123.456-5 \
        TOTAL $ 100.000";
    let e = s().desde_texto(invertida);
    assert_eq!(e.rut_deudor.unwrap().valor, "77123456-5");
}

#[test]
fn el_rut_no_se_confunde_con_un_monto() {
    // "76.702.579-3" tiene la forma de 76.702.579 pesos. Sin enmascarar los RUT
    // antes de buscar cifras, el respaldo del monto se lleva el RUT del emisor.
    let sin_etiqueta = "R.U.T.: 76.702.579-3 FACTURA ELECTRONICA N° 8782 \
                        SEÑOR(ES): ACME SPA R.U.T.: 96.707.720-8 Valor $ 883.456";
    let e = s().desde_texto(sin_etiqueta);
    assert_eq!(e.monto_total.unwrap().valor, "883456");
}

#[test]
fn la_razon_social_no_se_lleva_el_rut_pegado() {
    // Forma real medida: la factura escribe "RUT" sin dos puntos. El cotizador
    // analizado exigía los dos puntos y devolvía
    // "CENCOSUD RETAIL S.A.   RUT   81.201.000-K".
    let e = s().desde_texto(
        "R.U.T.: 77.394.491-1 FACTURA ELECTRONICA N° 425 \
         SEÑOR(ES): CENCOSUD RETAIL S.A.   RUT   81.201.000-K \
         TOTAL $ 7.942.279",
    );
    assert_eq!(e.razon_social_deudor.unwrap().valor, "CENCOSUD RETAIL S.A.");
    assert_eq!(e.rut_deudor.unwrap().valor, "81201000-K");
}

#[test]
fn un_bloque_senores_partido_no_deja_una_razon_social_de_basura() {
    // El cotizador analizado devolvió `"s):"` en una factura cuyo "Señor(es)"
    // venía partido por el extractor de texto. Mejor nada que basura: aguas
    // arriba, un nombre inventado es peor que un campo vacío.
    let e = s().desde_texto("Señor (es): R.U.T.: 76.362.176-6 TOTAL $ 252.280");
    assert!(e.razon_social_deudor.is_none(), "se esperaba ninguno, vino {:?}", e.razon_social_deudor);
}

#[test]
fn un_pdf_escaneado_no_devuelve_nada_en_vez_de_inventar() {
    let e = s().desde_texto("");
    assert!(e.folio.is_none() && e.monto_total.is_none());
    assert_eq!(e.origen_dominante(), None);
}

#[test]
fn la_fecha_en_palabras_tambien_se_entiende() {
    let e = s().desde_texto("Fecha de Emisión: 16 de septiembre de 2026 TOTAL $ 1.000");
    assert_eq!(e.fecha_emision.unwrap().valor, "2026-09-16");
}

// ── El contraste entre capas ──────────────────────────────────────────────────

use worker_storage_processor::domain::models::extraccion_factura_model::ExtraccionFactura;
use worker_storage_processor::domain::models::ted_model::{LecturaTimbre, Ted, Verificacion};

fn timbre(monto: i64, verificacion: Verificacion) -> LecturaTimbre {
    LecturaTimbre {
        ted: Ted {
            rut_emisor: "76702579-3".into(),
            tipo_dte: 33,
            folio: 8782,
            fecha_emision: "2026-09-30".into(),
            rut_receptor: "96707720-8".into(),
            razon_social_receptor: "MEDITERRANEAN SHIPPING COMPANY".into(),
            monto_total: monto,
            primer_item: None,
        },
        verificacion,
    }
}

#[test]
fn el_timbre_le_gana_a_la_capa_de_texto() {
    let e = s().combinar(
        Some(ExtraccionFactura::from(&timbre(883_456, Verificacion::Verificado))),
        s().desde_texto(FACTURA),
    );
    assert_eq!(e.monto_total.as_ref().unwrap().valor, "883456");
    assert_eq!(e.monto_total.as_ref().unwrap().origen, OrigenDatos::TimbreVerificado);
    assert_eq!(e.origen_dominante(), Some(OrigenDatos::TimbreVerificado));
    assert!(e.es_confiable());
}

#[test]
fn una_capa_de_texto_superpuesta_queda_denunciada() {
    // El ataque medido: la imagen es una factura real con su timbre válido, y
    // encima va una capa de texto que declara otro monto. Los dos cotizadores
    // analizados declararon 88.354.000 donde el timbre dice 883.456, porque
    // ninguno mira las dos capas.
    let adulterada = FACTURA.replace("TOTAL $ 883.456", "TOTAL $ 88.354.000");
    let e = s().combinar(
        Some(ExtraccionFactura::from(&timbre(883_456, Verificacion::Verificado))),
        s().desde_texto(&adulterada),
    );

    assert_eq!(e.monto_total.as_ref().unwrap().valor, "883456", "manda el timbre");
    assert_eq!(e.discrepancias.len(), 1);
    let d = &e.discrepancias[0];
    assert_eq!(d.campo, "monto_total");
    assert_eq!((d.segun_timbre.as_str(), d.segun_texto.as_str()), ("883456", "88354000"));
    assert!(!e.es_confiable(), "con una capa que contradice al timbre no se publica sin mirar");
}

#[test]
fn sin_timbre_manda_el_texto_pero_marcado_como_tal() {
    let e = s().combinar(None, s().desde_texto(FACTURA));
    assert_eq!(e.monto_total.as_ref().unwrap().origen, OrigenDatos::CapaDeTexto);
    assert!(!e.es_confiable(), "sin firma no hay nada que confiar");
    assert_eq!(e.verificacion, None);
}

#[test]
fn el_texto_rellena_lo_que_el_timbre_no_trae() {
    // El TED no lleva fecha de vencimiento, y acá tampoco el texto: lo que se
    // prueba es que un campo ausente en el timbre se completa con el texto en
    // vez de perderse. Se usa la razón social, que el TED trunca a 40.
    let mut t = ExtraccionFactura::from(&timbre(883_456, Verificacion::Verificado));
    t.razon_social_deudor = None;
    let e = s().combinar(Some(t), s().desde_texto(FACTURA));
    let r = e.razon_social_deudor.as_ref().unwrap();
    assert_eq!(r.valor, "MEDITERRANEAN SHIPPING COMPANY");
    assert_eq!(r.origen, OrigenDatos::CapaDeTexto, "y queda marcado de dónde salió");
}

#[test]
fn un_timbre_sin_firma_verificada_no_es_confiable_aunque_todo_calce() {
    let e = s().combinar(
        Some(ExtraccionFactura::from(&timbre(883_456, Verificacion::FirmaInvalida))),
        s().desde_texto(FACTURA),
    );
    assert!(e.discrepancias.is_empty(), "las capas coinciden");
    assert!(!e.es_confiable(), "pero la firma no verificó");
    assert_eq!(e.origen_dominante(), Some(OrigenDatos::TimbreSinVerificar));
}

#[test]
fn una_boleta_no_se_puede_ceder() {
    let mut l = timbre(883_456, Verificacion::Verificado);
    l.ted.tipo_dte = 39; // boleta electrónica
    let e = ExtraccionFactura::from(&l);
    assert_eq!(e.es_cedible, Some(false));
}

#[test]
fn el_rut_puede_venir_antes_del_nombre() {
    // Forma real medida: `Señor(es): R.U.T. 76.362.176-6 Besalco Piques Y
    // Tuneles S.A. LAS CONDES , Santiago`. El nombre va DESPUÉS del RUT, y
    // detrás viene la dirección pegada.
    let e = s().desde_texto(
        "COMERCIALIZADORA DIMAT SPA R.U.T. 78.104.127-0 FACTURA ELECTRÓNICA N° 1726 \
         Señor(es): R.U.T. 76.362.176-6 Besalco Piques Y Tuneles S.A. LAS CONDES , Santiago \
         Giro Construcción",
    );
    assert_eq!(e.rut_deudor.unwrap().valor, "76362176-6");
    assert_eq!(e.razon_social_deudor.unwrap().valor, "Besalco Piques Y Tuneles S.A.");
}

#[test]
fn total_no_es_el_encabezado_de_la_columna_de_detalle() {
    // Forma real medida: la tabla de detalle tiene una columna "Total" y la
    // primera fila empieza con su número de orden, así que el primer candidato
    // tras la etiqueta es un "1". El total de verdad está etiquetado
    // "MONTO TOTAL".
    let e = s().desde_texto(
        "RUT: 77.394.491-1 FACTURA ELECTRÓNICA Nº 425 Señor(es) CENCOSUD RETAIL S.A. \
         RUT 81.201.000-K Nº Descripción Cant/Unidad Precio Unit. Imp/Ret Ind Total \
         1 1ERA QUINCENA AGOSTO 1 $6.674.184 AF $6.674.184 \
         Monto Neto $6.674.184 Monto Exento $0 19% IVA $1.268.095 MONTO TOTAL $7.942.279",
    );
    assert_eq!(e.monto_total.unwrap().valor, "7942279");
}

#[test]
fn con_las_etiquetas_separadas_de_sus_valores_no_se_inventa_un_monto() {
    // Forma real medida: una factura a dos columnas donde el extractor entrega
    // los valores ANTES que sus etiquetas — `Exento 40.280 252.280 19% I.V.A.
    // TOTAL`. El TOTAL no tiene número detrás. Dejarlo vacío es correcto: el
    // dato existe en el timbre, y entre no saber e inventar, no saber.
    let e = s().desde_texto(
        "FACTURA ELECTRÓNICA N° 1726 Señor(es): R.U.T. 76.362.176-6 Besalco S.A. \
         Son : Neto 212.000 Exento 40.280 252.280 19% I.V.A. TOTAL",
    );
    assert!(e.monto_total.is_none(), "vino {:?}", e.monto_total);
    assert_eq!(e.folio.unwrap().valor, "1726");
}
