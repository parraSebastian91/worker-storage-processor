//! Laboratorio de lectura del timbre.
//!
//! Corre el mismo `TimbreManagerService` que va a correr el worker, pero sobre
//! una carpeta de imágenes, sin RabbitMQ ni MinIO ni base de datos. Sirve para
//! medir contra facturas reales y ajustar.
//!
//!     cargo run --features timbre --bin lab-timbre -- <carpeta|archivo>…
//!     cargo run --features timbre --bin lab-timbre -- --json <carpeta>
//!
//! Toma PNG/JPG. Los PDF hay que renderizarlos antes: en Docker el worker usa
//! pdfium, y fuera de Docker `pypdfium2` es el MISMO motor, así que renderizar
//! con eso es fiel y no una aproximación:
//!
//!     python3 -c "import pypdfium2 as p; d=p.PdfDocument('f.pdf'); \
//!       pg=d[0]; pg.render(scale=3000/pg.get_width()).to_pil().save('f.png')"

use std::path::{Path, PathBuf};
use std::time::Instant;

use worker_storage_processor::aplication::service::timbre_manager_service::TimbreManagerService;
use worker_storage_processor::domain::models::ted_model::Verificacion;

fn imagenes(rutas: &[String]) -> Vec<PathBuf> {
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
        matches!(
            p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
            Some("png") | Some("jpg") | Some("jpeg")
        )
    });
    salida.sort();
    salida
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    let rutas: Vec<String> = args.into_iter().filter(|a| !a.starts_with("--")).collect();
    if rutas.is_empty() {
        eprintln!("uso: lab-timbre [--json] <carpeta|imagen>…");
        std::process::exit(2);
    }

    let servicio = TimbreManagerService::new();
    let archivos = imagenes(&rutas);
    let (mut leidos, mut verificados) = (0usize, 0usize);
    let mut filas: Vec<String> = Vec::new();

    if !json {
        println!("{:<44} {:>4} {:>8} {:>12} {:>7}  {}",
                 "archivo", "TD", "folio", "monto", "ms", "estado");
        println!("{}", "─".repeat(104));
    }

    for ruta in &archivos {
        let nombre = ruta.file_name().unwrap().to_string_lossy().to_string();
        let t = Instant::now();
        let img = match image::open(ruta) {
            Ok(i) => i,
            Err(e) => { println!("{nombre:<44} no se pudo abrir: {e}"); continue; }
        };
        let lectura = servicio.leer(&img);
        let ms = t.elapsed().as_millis();

        match lectura {
            Some(l) => {
                leidos += 1;
                if matches!(
                    l.verificacion,
                    Verificacion::Verificado | Verificacion::VerificadoConReparacion
                ) {
                    verificados += 1;
                }
                let estado = match l.verificacion {
                    Verificacion::Verificado => "✅ firma verificada",
                    Verificacion::VerificadoConReparacion => "✅ verificada (símbolo con un carácter corrupto)",
                    Verificacion::FirmaInvalida => "⚠️  FIRMA INVÁLIDA",
                    Verificacion::SinClavePublica => "⚠️  CAF sin clave pública",
                    Verificacion::TedIncompleto => "⚠️  TED incompleto",
                };
                let cedible = if l.ted.es_cedible() { "" } else { "  ⛔ NO CEDIBLE" };
                if json {
                    filas.push(format!(
                        r#"{{"archivo":"{}","leido":true,"td":{},"folio":{},"monto":{},"rut_receptor":"{}","verificacion":"{:?}","cedible":{},"ms":{}}}"#,
                        nombre, l.ted.tipo_dte, l.ted.folio, l.ted.monto_total,
                        l.ted.rut_receptor, l.verificacion, l.ted.es_cedible(), ms));
                } else {
                    println!("{:<44} {:>4} {:>8} {:>12} {:>7}  {}{}",
                             recortar(&nombre, 44), l.ted.tipo_dte, l.ted.folio,
                             l.ted.monto_total, ms, estado, cedible);
                    println!("{:<44} {} · {}", "", l.ted.rut_receptor,
                             recortar(&l.ted.razon_social_receptor, 46));
                }
            }
            None => {
                if json {
                    filas.push(format!(r#"{{"archivo":"{nombre}","leido":false,"ms":{ms}}}"#));
                } else {
                    println!("{:<44} {:>4} {:>8} {:>12} {:>7}  ❌ sin timbre legible",
                             recortar(&nombre, 44), "—", "—", "—", ms);
                }
            }
        }
    }

    if json {
        println!("[{}]", filas.join(","));
    } else {
        let n = archivos.len();
        println!("\n{leidos}/{n} con timbre legible · {verificados}/{leidos} con firma verificada");
        if leidos < n {
            println!("\nLos que no decodifican caen al OCR, que es el comportamiento previsto:\n\
                      hay respaldos que legítimamente no llevan timbre (guías, cotizaciones,\n\
                      órdenes de compra).");
        }
    }
}

fn recortar(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}
