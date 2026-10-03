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
    // `--vectorial` prueba la reconstrucción del símbolo desde los rectángulos.
    let vectorial = todos.iter().any(|a| a == "--vectorial");
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
        if vectorial {
            if let Ok(cajas) = documentos.rectangulos_primera_pagina(&bytes) {
                let mut anchos: Vec<f32> = cajas.iter().map(|c| c[2] - c[0]).collect();
                let mut altos: Vec<f32> = cajas.iter().map(|c| c[3] - c[1]).collect();
                anchos.sort_by(|a, b| a.partial_cmp(b).unwrap());
                altos.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let q = |v: &Vec<f32>, p: f32| v[((v.len() as f32 - 1.0) * p) as usize];
                println!(
                    "   {} rects · ancho min {:.3} p25 {:.3} mediana {:.3} max {:.3} · alto min {:.3} mediana {:.3} max {:.3}",
                    cajas.len(), anchos[0], q(&anchos, 0.25), q(&anchos, 0.5), anchos[anchos.len()-1],
                    altos[0], q(&altos, 0.5), altos[altos.len()-1]
                );
                // Alturas distintas que aparecen, redondeadas: dicen si hay una
                // sola altura de fila o varias.
                let mut vistas: Vec<String> = Vec::new();
                for a in &altos {
                    let r = format!("{:.2}", a);
                    if !vistas.contains(&r) { vistas.push(r); }
                    if vistas.len() > 12 { break; }
                }
                println!("   alturas distintas: {}", vistas.join(" "));
                // Posiciones distintas: con un rectángulo por módulo, esto da
                // filas y columnas exactas, sin estimarlas desde el bounding box.
                let mut xs: Vec<i64> = cajas.iter().map(|c| (c[0] * 100.0).round() as i64).collect();
                let mut ys: Vec<i64> = cajas.iter().map(|c| (c[1] * 100.0).round() as i64).collect();
                xs.sort_unstable(); xs.dedup();
                ys.sort_unstable(); ys.dedup();
                println!("   posiciones X distintas: {} · Y distintas: {}", xs.len(), ys.len());
                if ys.len() > 1 {
                    let mut pasos: Vec<i64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
                    pasos.sort_unstable(); pasos.dedup();
                    println!("   saltos entre filas (centésimas de punto): {:?}", &pasos[..pasos.len().min(8)]);
                }
                if xs.len() > 1 {
                    let mut pasos: Vec<i64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
                    pasos.sort_unstable(); pasos.dedup();
                    println!("   saltos entre columnas: {:?}", &pasos[..pasos.len().min(8)]);
                }
            }
            match documentos.simbolo_vectorial_primera_pagina(&bytes) {
                Ok(Some(sim)) => {
                    let _ = sim.save(format!("/tmp/vectorial-{}.png", nombre(&ruta).replace(['/', ' '], "_")));
                    let t0 = Instant::now();
                    let lectura = timbres.leer(&sim);
                    let ms = t0.elapsed().as_millis();
                    match lectura {
                        Some(l) => println!(
                            "{:<42} {}x{} → folio {} monto {} · {:?} ({ms} ms)",
                            nombre(&ruta), sim.width(), sim.height(),
                            l.ted.folio, l.ted.monto_total, l.verificacion
                        ),
                        None => println!(
                            "{:<42} {}x{} reconstruido, pero NO decodifica ({ms} ms)",
                            nombre(&ruta), sim.width(), sim.height()
                        ),
                    }
                }
                Ok(None) => println!("{:<42} sin rectángulos suficientes", nombre(&ruta)),
                Err(e) => println!("{:<42} error: {e}", nombre(&ruta)),
            }
            continue;
        }
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
            let crudo = documentos
                .simbolo_vectorial_primera_pagina(&bytes)
                .ok()
                .flatten()
                .and_then(|sim| timbres.decodificar(&sim))
                .or_else(|| imagenes.iter().find_map(|i| timbres.decodificar(i)))
                .or_else(|| {
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
