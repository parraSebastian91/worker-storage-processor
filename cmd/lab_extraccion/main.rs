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
    let args: Vec<String> = todos.into_iter().filter(|a| !a.starts_with("--")).collect();
    if args.is_empty() {
        eprintln!("uso: lab-extraccion <carpeta|pdf>…");
        std::process::exit(2);
    }

    let documentos = DocumentManagerService::new();
    let timbres = TimbreManagerService::new();
    let extraccion = ExtraccionFacturaService::new();

    println!(
        "{:<42} {:>7} {:>9} {:>14} {:>9}  {}",
        "archivo", "ms", "folio", "monto", "origen", "deudor"
    );
    println!("{}", "─".repeat(118));

    let (mut confiables, mut con_discrepancia) = (0usize, 0usize);
    for ruta in pdfs(&args) {
        let bytes = match std::fs::read(&ruta) {
            Ok(b) => b,
            Err(e) => {
                println!("{:<42} error al leer: {e}", nombre(&ruta));
                continue;
            }
        };
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
