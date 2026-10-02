//! Laboratorio de extracción en capas.
//!
//! Corre la misma cascada que corre el worker —timbre, capa de texto,
//! contraste— sobre PDFs sueltos, sin RabbitMQ ni MinIO ni base de datos.
//! Hermano de `lab-timbre`, que mira sólo la capa 1 y toma imágenes.
//!
//!     PDFIUM_PATH=/ruta/libpdfium.dylib \
//!       cargo run --features "pdf,timbre" --bin lab-extraccion -- <carpeta|pdf>…
//!
//! El OCR no participa: vive detrás de la feature `ocr`, que arrastra tesseract
//! y leptonica. Lo que se mide acá es justamente lo que se puede medir sin eso.

use std::path::{Path, PathBuf};
use std::time::Instant;

use worker_storage_processor::aplication::service::{
    document_manager_service::DocumentManagerService,
    extraccion_factura_service::ExtraccionFacturaService,
    timbre_manager_service::TimbreManagerService,
};
use worker_storage_processor::domain::models::ted_model::OrigenDatos;

fn pdfs(rutas: &[String]) -> Vec<PathBuf> {
    let mut salida = Vec::new();
    for r in rutas {
        let p = Path::new(r);
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                salida.extend(rd.filter_map(|e| e.ok().map(|e| e.path())));
            }
        } else {
            salida.push(p.to_path_buf());
        }
    }
    salida.retain(|p| {
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() == Some("pdf")
    });
    salida.sort();
    salida
}

fn marca(o: Option<OrigenDatos>) -> &'static str {
    match o {
        Some(OrigenDatos::TimbreVerificado) => "timbre✅",
        Some(OrigenDatos::TimbreSinVerificar) => "timbre⚠",
        Some(OrigenDatos::CapaDeTexto) => "texto",
        Some(OrigenDatos::Ocr) => "ocr",
        None => "—",
    }
}

fn main() {
    let todos: Vec<String> = std::env::args().skip(1).collect();
    // `--texto` vuelca la capa de texto cruda. Es la herramienta con la que se
    // ajustan las regex: sin ver lo que pdfium entrega, cada corrección es una
    // adivinanza.
    let volcar = todos.iter().any(|a| a == "--texto");
    // `--json` para medir un corpus grande sin parsear una tabla de ancho fijo.
    let json = todos.iter().any(|a| a == "--json");
    // `--ted` vuelca el XML del timbre tal como salió del PDF417, sin
    // interpretar. Es lo único que permite distinguir una firma que no verifica
    // porque el documento fue adulterado de una que no verifica porque nosotros
    // leímos mal un byte.
    let ted_crudo = todos.iter().any(|a| a == "--ted");
    // `--imgs` lista las imágenes embebidas y si cada una lleva el timbre. Es
    // lo que dice por qué una factura cae al render de página, que cuesta un
    // orden de magnitud más.
    let imgs = todos.iter().any(|a| a == "--imgs");
    let args: Vec<String> = todos.into_iter().filter(|a| !a.starts_with("--")).collect();
    if args.is_empty() {
        eprintln!("uso: lab-extraccion <carpeta|pdf>…");
        std::process::exit(2);
    }

    let documentos = DocumentManagerService::new();
    let timbres = TimbreManagerService::new();
    let extraccion = ExtraccionFacturaService::new();

    if !json && !volcar {
        println!(
            "{:<42} {:>7} {:>9} {:>14} {:>9}  {}",
            "archivo", "ms", "folio", "monto", "origen", "deudor"
        );
        println!("{}", "─".repeat(118));
    }
    if json {
        println!("[");
    }
    let mut primero = true;

    let (mut confiables, mut con_discrepancia) = (0usize, 0usize);
    for ruta in pdfs(&args) {
        let bytes = match std::fs::read(&ruta) {
            Ok(b) => b,
            Err(e) => {
                println!("{:<42} error al leer: {e}", nombre(&ruta));
                continue;
            }
        };
        if imgs {
            let v = documentos.imagenes_embebidas_primera_pagina(&bytes).unwrap_or_default();
            let (paths, top, textos) =
                documentos.conteo_objetos_primera_pagina(&bytes).unwrap_or((0, 0, 0));
            print!(
                "{:<42} objetos: {paths} paths, {top} imgs nivel 1, {textos} textos | embebidas: {}",
                nombre(&ruta),
                v.len()
            );
            for (i, im) in v.iter().enumerate() {
                let ok = timbres.decodificar(im).is_some();
                print!("  [{i}] {}x{}{}", im.width(), im.height(), if ok { " ⇐ TIMBRE" } else { "" });
            }
            println!();
            continue;
        }
        if ted_crudo {
            let imagenes =
                documentos.imagenes_embebidas_primera_pagina(&bytes).unwrap_or_default();
            let crudo = imagenes.iter().find_map(|i| timbres.decodificar(i)).or_else(|| {
                documentos
                    .render_first_page_png_from_pdf(&bytes)
                    .ok()
                    .and_then(|png| image::load_from_memory(&png).ok())
                    .and_then(|img| timbres.decodificar(&img))
            });
            match crudo {
                Some(t) => println!("\n── {} ──\n{}\n", nombre(&ruta), t),
                None => println!("\n── {} ── sin timbre legible\n", nombre(&ruta)),
            }
            continue;
        }
        if volcar {
            match documentos.texto_de_capa(&bytes) {
                Ok(paginas) => {
                    let p1 = paginas.first().cloned().unwrap_or_default();
                    let plano = p1.split_whitespace().collect::<Vec<_>>().join(" ");
                    println!("\n── {} ──\n{}\n", nombre(&ruta), plano);
                }
                Err(e) => println!("\n── {} ── sin capa de texto: {e}\n", nombre(&ruta)),
            }
            continue;
        }
        let t0 = Instant::now();
        let e = extraccion.extraer_de_pdf(&bytes, &documentos, &timbres);
        let ms = t0.elapsed().as_millis();

        if e.es_confiable() {
            confiables += 1;
        }
        if !e.discrepancias.is_empty() {
            con_discrepancia += 1;
        }

        if json {
            // El conteo de objetos sólo se pide en modo json: cuesta otra
            // apertura del PDF y únicamente sirve para el diagnóstico.
            let (paths, imgs, textos) =
                documentos.conteo_objetos_primera_pagina(&bytes).unwrap_or((0, 0, 0));
            let caracteres_texto = documentos
                .texto_de_capa(&bytes)
                .ok()
                .and_then(|p| p.first().map(|t| t.split_whitespace().count()))
                .unwrap_or(0);
            if !primero {
                println!(",");
            }
            primero = false;
            print!(
                "  {{\"archivo\":{},\"ms\":{},\"origen\":\"{}\",\"verificacion\":\"{:?}\",\
                 \"folio\":{},\"monto\":{},\"deudor\":{},\"tipo_dte\":{},\
                 \"discrepancias\":{},\"paths\":{},\"imagenes\":{},\"objetos_texto\":{},\
                 \"palabras_capa_texto\":{}}}",
                escapar(&nombre(&ruta)),
                ms,
                marca(e.origen_dominante()),
                e.verificacion,
                e.folio.as_ref().map(|c| escapar(&c.valor)).unwrap_or("null".into()),
                e.monto_total.as_ref().map(|c| escapar(&c.valor)).unwrap_or("null".into()),
                e.razon_social_deudor.as_ref().map(|c| escapar(&c.valor)).unwrap_or("null".into()),
                e.tipo_dte.map(|t| t.to_string()).unwrap_or("null".into()),
                e.discrepancias.len(),
                paths,
                imgs,
                textos,
                caracteres_texto,
            );
            continue;
        }

        println!(
            "{:<42} {:>7} {:>9} {:>14} {:>9}  {}",
            nombre(&ruta),
            ms,
            e.folio.as_ref().map(|c| c.valor.as_str()).unwrap_or("—"),
            e.monto_total.as_ref().map(|c| c.valor.as_str()).unwrap_or("—"),
            marca(e.origen_dominante()),
            e.razon_social_deudor.as_ref().map(|c| c.valor.as_str()).unwrap_or("—"),
        );
        for d in &e.discrepancias {
            println!(
                "{:>42}   ⚠ {} — timbre dice {}, el texto dice {}",
                "", d.campo, d.segun_timbre, d.segun_texto
            );
        }
    }

    if json {
        println!("\n]");
        return;
    }
    println!(
        "\n{confiables} con timbre verificado y sin contradicción · \
         {con_discrepancia} con el texto contradiciendo al timbre"
    );
    if con_discrepancia > 0 {
        println!(
            "\nUna contradicción no es un error de lectura: el timbre está firmado y\n\
             la capa de texto no, así que superponerla es trivial. Ahí hay que mirar."
        );
    }
}

fn nombre(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().chars().take(41).collect()
}

/// Escapa una cadena como literal JSON. Las razones sociales traen comillas y
/// barras más seguido de lo que uno esperaría.
fn escapar(t: &str) -> String {
    let mut out = String::with_capacity(t.len() + 2);
    out.push('"');
    for c in t.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
