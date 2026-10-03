//! Extracción de los datos de una factura, en capas, con la procedencia pegada
//! a cada valor.
//!
//! El orden no es de velocidad sino de confianza, y después de medirlo resultó
//! que coinciden:
//!
//! | capa | qué es | medido |
//! |---|---|---|
//! | 1. Timbre (TED) | **firmado**: falsificarlo pide un CAF del SII | 35–188 ms |
//! | 2. Capa de texto | exacta pero **no firmada**: se superpone con cualquier editor | 6–61 ms |
//! | 3. OCR | adivinanza sobre píxeles | segundos |
//!
//! La capa 2 existía afuera y no acá: dos cotizadores ajenos la usaban como
//! única fuente y acertaban casi todo, mientras nosotros rasterizábamos para
//! pasarle Tesseract a un PDF que ya traía el texto en bytes. Las dos ideas
//! buenas que se les tomaron están marcadas abajo.
//!
//! Lo que esos cotizadores no hacen —y acá sí— es **contrastar las capas**.
//! Medido: un PDF con la imagen de una factura real y una capa de texto
//! superpuesta hace que los dos declaren 88.354.000 donde el timbre firmado dice
//! 883.456. Si las dos capas existen y no coinciden, eso se reporta.
//!
//! Análisis completo en `docs/general/findings/cotizadores-externos/`.

use regex::Regex;

use crate::domain::models::extraccion_factura_model::{Campo, Discrepancia, ExtraccionFactura};
use crate::domain::models::ted_model::OrigenDatos;

pub struct ExtraccionFacturaService {}

impl Default for ExtraccionFacturaService {
    fn default() -> Self {
        Self::new()
    }
}

/// Un RUT chileno: 7 u 8 dígitos con puntos opcionales, guion y dígito
/// verificador. El `\s?` antes del DV es necesario: varios facturadores lo
/// separan.
const RUT: &str = r"\d{1,2}\.?\d{3}\.?\d{3}\s?-\s?[\dkK]";

/// Un monto. **Acepta el cero suelto a propósito.**
///
/// El patrón de uno de los cotizadores analizados exigía separador de miles, así
/// que no podía representar un `0`; ante `IMPUESTO ADICIONAL $ 0 TOTAL $
/// 3.708.040` no matcheaba el cero, seguía consumiendo y se llevaba el total —
/// que después sumaba, duplicando el monto de la factura. Una regex que no puede
/// expresar el valor neutro de su campo no falla devolviendo nada: falla
/// devolviendo lo que venga después.
const MONTO: &str = r"(\d{1,3}(?:[.\s]\d{3})+|\d+)";

impl ExtraccionFacturaService {
    pub fn new() -> Self {
        Self {}
    }

    /// Campos de la factura leídos de la capa de texto del PDF.
    ///
    /// Pura: recibe texto y devuelve campos. Se puede probar sin pdfium, sin
    /// tesseract y sin una factura real.
    pub fn desde_texto(&self, texto: &str) -> ExtraccionFactura {
        let t = Regex::new(r"\s+").unwrap().replace_all(texto, " ").to_string();
        let o = OrigenDatos::CapaDeTexto;
        let mut e = ExtraccionFactura::default();

        e.folio = Self::folio(&t).map(|v| Campo::nuevo(v, o));

        // Idea tomada del cotizador Astro: los RUT no se distinguen por su
        // forma, se distinguen por **dónde están**. El bloque "SEÑOR(ES)" es
        // donde la factura chilena identifica a quién se le factura, así que
        // todo RUT anterior es del emisor y todo RUT posterior, del receptor.
        // Lo que hacíamos antes —"el primer RUT que coincida"— acierta sólo si
        // el PDF los imprime en el orden habitual.
        let corte = Self::indice_senores(&t);
        let ruts: Vec<(usize, String)> = Regex::new(RUT)
            .unwrap()
            .find_iter(&t)
            .map(|m| (m.start(), Self::normalizar_rut(m.as_str())))
            .collect();

        match corte {
            Some(i) => {
                e.rut_emisor = ruts.iter().find(|(p, _)| *p < i).map(|(_, r)| Campo::nuevo(r, o));
                e.rut_deudor = ruts.iter().find(|(p, _)| *p >= i).map(|(_, r)| Campo::nuevo(r, o));
            }
            None => {
                e.rut_emisor = ruts.first().map(|(_, r)| Campo::nuevo(r, o));
                e.rut_deudor = ruts.get(1).map(|(_, r)| Campo::nuevo(r, o));
            }
        }

        e.razon_social_deudor =
            corte.and_then(|i| Self::razon_social(&t[i..])).map(|v| Campo::nuevo(v, o));
        e.monto_total = Self::monto(&t).map(|v| Campo::nuevo(v.to_string(), o));
        e.fecha_emision = Self::fecha_emision(&t).map(|v| Campo::nuevo(v, o));
        e
    }

    fn indice_senores(t: &str) -> Option<usize> {
        Regex::new(r"(?i)Se[ñn]or\s*\(?e?s?\)?|Sr\.?\s*\(es\)")
            .unwrap()
            .find(t)
            .map(|m| m.start())
    }

    fn normalizar_rut(r: &str) -> String {
        r.replace([' ', '.'], "").to_uppercase()
    }

    fn folio(t: &str) -> Option<String> {
        let patrones = [
            r"(?i)FACTURA(?:\s+NO\s+AFECTA\s+O\s+EXENTA)?\s+ELECTR[ÓO]NICA[^0-9]{0,150}?N[°ºo*]?\.?\s*:?\s*(\d{1,10})",
            r"(?i)\bFolio\s*[:\s]\s*(\d{1,10})\b",
            r"(?i)\bN[°ºo*]\.?\s*:?\s*(\d{3,10})\b",
        ];
        patrones.iter().find_map(|p| {
            Regex::new(p).unwrap().captures(t).map(|c| c[1].to_string())
        })
    }

    /// El TOTAL de la factura, nunca el neto.
    ///
    /// Se busca la etiqueta y se toma el **primer** número que la siga de cerca.
    /// No se calcula `neto + IVA + adicional`: ese camino suma tres lecturas
    /// falibles en vez de una, y en las facturas medidas falló por un motivo que
    /// no tenía nada que ver con la calidad de la lectura (ver `MONTO`).
    fn monto(t: &str) -> Option<i64> {
        // Enmascarar los RUT antes de buscar cifras — idea del otro cotizador.
        // Un RUT es ocho dígitos con puntos: sin esto, "76.702.579-3" se lee
        // como un monto de 76.702.579.
        let limpio = Regex::new(RUT).unwrap().replace_all(t, " ").to_string();

        // Primero las etiquetas que sólo pueden significar el total de la
        // factura. `MONTO TOTAL` es la que usa la mayoría de los facturadores y
        // es inequívoca.
        let fuerte = Regex::new(&format!(
            r"(?i)\b(?:MONTO\s+TOTAL|TOTAL\s+A\s+PAGAR|VALOR\s+TOTAL|TOTAL\s+GENERAL)\s*:?\s*\$?\s*{MONTO}"
        ))
        .unwrap();
        if let Some(c) = fuerte.captures(&limpio) {
            return Self::a_entero(&c[1]);
        }

        // Si no está, `TOTAL` a secas. Sin lookahead —el `regex` de Rust no lo
        // tiene— se capturan todas las etiquetas parecidas y se filtra por cuál
        // es; las variantes largas van primero para que "TOTAL NETO" gane sobre
        // "TOTAL" en la misma posición.
        //
        // Y de los candidatos se toma el **mayor**, no el primero: "Total" es
        // también el encabezado de la columna de la tabla de detalle, y lo que
        // le sigue es el número de la primera fila. Medido: una factura real
        // trae `… Imp/Ret Ind Total 1 1ERA QUINCENA AGOSTO …` y el primer
        // candidato era `1`.
        let etiquetas = Regex::new(&format!(
            r"(?i)\b(MONTO\s+NETO|TOTAL\s+NETO|TOTAL\s+EXENTO|SUB\s*TOTAL|TOTAL)\s*:?\s*(\$\s*)?{MONTO}"
        ))
        .unwrap();
        let mejor = etiquetas
            .captures_iter(&limpio)
            .filter(|c| {
                Regex::new(r"\s+").unwrap().replace_all(&c[1], " ").to_uppercase() == "TOTAL"
            })
            // Exigir `$` o separador de miles: descarta el `1` del encabezado de
            // columna sin descartar un total legítimo, que siempre trae uno u
            // otro.
            .filter(|c| c.get(2).is_some() || c[3].contains(['.', ' ']))
            .filter_map(|c| Self::a_entero(&c[3]))
            .max();
        if mejor.is_some() {
            return mejor;
        }

        // Respaldo: la cifra con "$" más alta. Es lo que hacen los dos
        // cotizadores analizados cuando no encuentran la etiqueta, y es una
        // conjetura: el campo queda marcado como capa de texto igual, así que
        // aguas arriba no se confunde con un dato del timbre.
        let con_peso = Regex::new(&format!(r"\$\s*{MONTO}")).unwrap();
        con_peso
            .captures_iter(&limpio)
            .filter_map(|c| Self::a_entero(&c[1]))
            .max()
    }

    fn a_entero(s: &str) -> Option<i64> {
        s.replace(['.', ' '], "").parse::<i64>().ok().filter(|n| *n > 0)
    }

    fn fecha_emision(t: &str) -> Option<String> {
        let ventana = Regex::new(r"(?i)Fecha\s*(?:de\s*)?Emisi[óo]n\s*:?\s*(.{0,30})")
            .unwrap()
            .captures(t)?
            .get(1)?
            .as_str()
            .to_string();
        Self::fecha(&ventana)
    }

    fn fecha(t: &str) -> Option<String> {
        const MESES: [(&str, &str); 13] = [
            ("enero", "01"), ("febrero", "02"), ("marzo", "03"), ("abril", "04"),
            ("mayo", "05"), ("junio", "06"), ("julio", "07"), ("agosto", "08"),
            ("septiembre", "09"), ("setiembre", "09"), ("octubre", "10"),
            ("noviembre", "11"), ("diciembre", "12"),
        ];
        if let Some(c) = Regex::new(r"(\d{4})-(\d{1,2})-(\d{1,2})").unwrap().captures(t) {
            return Self::armar(&c[1], &c[2], &c[3]);
        }
        if let Some(c) = Regex::new(r"(?i)(\d{1,2})\s+de\s+([a-zA-ZÀ-ÿ]+)\s+(?:de|del)\s+(\d{4})")
            .unwrap()
            .captures(t)
        {
            let mes = c[2].to_lowercase();
            let mm = MESES.iter().find(|(n, _)| *n == mes)?.1;
            return Self::armar(&c[3], mm, &c[1]);
        }
        let c = Regex::new(r"(\d{1,2})[/\-](\d{1,2})[/\-](\d{4})").unwrap().captures(t)?;
        Self::armar(&c[3], &c[2], &c[1])
    }

    fn armar(a: &str, m: &str, d: &str) -> Option<String> {
        let (m, d) = (m.parse::<u32>().ok()?, d.parse::<u32>().ok()?);
        if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        Some(format!("{a}-{m:02}-{d:02}"))
    }

    /// La razón social del deudor es lo que sigue a "SEÑOR(ES)".
    ///
    /// El recorte del RUT no exige los dos puntos: un cotizador analizado los
    /// pedía (`R.U.T.:`) para no cortar nombres que contienen "RUTA" o "RUTH", y
    /// el resultado fue dejar el RUT pegado al nombre cuando la factura escribe
    /// "RUT" a secas — `"CENCOSUD RETAIL S.A.   RUT   81.201.000-K"`. Acá se
    /// corta ante cualquier forma de "RUT" **seguida de un RUT**, que es lo que
    /// de verdad distingue la etiqueta del comienzo de una palabra.
    fn razon_social(bloque: &str) -> Option<String> {
        let sin_etiqueta = Regex::new(r"(?i)^\s*Se[ñn]or\s*\(?e?s?\)?\s*:?\s*|^\s*Sr\.?\s*\(es\)\s*:?\s*")
            .unwrap()
            .replace(&bloque[..bloque.len().min(300)], "")
            .to_string();

        let corte = Regex::new(&format!(r"(?i)\s*R\.?\s?U\.?\s?T\.?\s*:?\s*{RUT}"))
            .unwrap()
            .find(&sin_etiqueta)
            .map(|m| m.start())
            .unwrap_or(sin_etiqueta.len());

        let limpiar = |t: &str| -> String {
            t.split(|c| c == '\n' || c == '\r')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(|c: char| c == ':' || c == '-' || c.is_whitespace())
                .to_string()
        };

        // Lo habitual es `SEÑOR(ES): NOMBRE R.U.T.: 12.345.678-9`, y el nombre es
        // lo que va antes del RUT. Pero hay facturadores que invierten el orden
        // —`Señor(es): R.U.T. 76.362.176-6 Besalco Piques Y Tuneles S.A.`— y ahí
        // lo de antes está vacío. En ese caso el nombre es lo que sigue al RUT.
        let antes = limpiar(&sin_etiqueta[..corte]);
        let nombre = if antes.is_empty() && corte < sin_etiqueta.len() {
            let despues = Regex::new(&format!(r"(?i)R\.?\s?U\.?\s?T\.?\s*:?\s*{RUT}"))
                .unwrap()
                .find(&sin_etiqueta)
                .map(|m| m.end())
                .unwrap_or(corte);
            limpiar(&sin_etiqueta[despues..])
        } else {
            antes
        };

        // Cortar en la siguiente etiqueta del formulario. Una razón social no
        // contiene "Giro" ni "TOTAL": si aparecen, ahí terminó el nombre y
        // empezó otro campo. Idea tomada del cotizador Astro, que limpia así
        // antes de buscar el nombre; sin esto, un bloque "Señor(es)" sin nombre
        // se lleva lo que venga detrás.
        let nombre = match Regex::new(
            r"(?i)\b(Giro|Direcci[oó]n|Ciudad|Comuna|Contacto|Vendedor|Tel[eé]fono|Fono|E-?mail|Correo|Cond\.?\s*Venta|Condiciones|Gu[ií]a|Fecha|Forma|Observaciones|Monto|Total|Neto|Exento|I\.?V\.?A\.?|S\.?I\.?I\.?)\b",
        )
        .unwrap()
        .find(&nombre)
        {
            Some(m) => nombre[..m.start()].trim().to_string(),
            None => nombre,
        };

        // Si el nombre trae su forma jurídica, ahí termina: lo que sigue es la
        // dirección o el giro. Medido: sin esto quedaba
        // "Besalco Piques Y Tuneles S.A. LAS CONDES , Santiago".
        let nombre = match Regex::new(r"(?i)\b(S\.?\s?A\.?|SpA|Ltda\.?|Limitada|E\.?I\.?R\.?L\.?)(?:\s|$)")
            .unwrap()
            .find(&nombre)
        {
            Some(m) => nombre[..m.end()].trim().to_string(),
            None => nombre,
        };

        // Dos caracteres no son una razón social. El cotizador que no filtraba
        // esto devolvió `"s):"` en una factura cuyo "Señor(es)" venía partido.
        if nombre.len() < 3 || !nombre.chars().any(|c| c.is_alphabetic()) {
            return None;
        }
        Some(nombre.chars().take(90).collect())
    }

    /// Mezcla lo que dijo el timbre con lo que dijo la capa de texto.
    ///
    /// El timbre gana siempre que tenga el campo. La capa de texto rellena lo
    /// que el timbre no trae —la fecha de vencimiento no está en el TED, por
    /// ejemplo— y además **se contrasta**: si las dos tienen el campo y no
    /// coinciden, queda anotado en `discrepancias`.
    pub fn combinar(
        &self,
        del_timbre: Option<ExtraccionFactura>,
        del_texto: ExtraccionFactura,
    ) -> ExtraccionFactura {
        let Some(mut base) = del_timbre else {
            return del_texto;
        };

        let mut discrepancias = Vec::new();
        let mut unir = |nombre: &str, a: &mut Option<Campo>, b: Option<Campo>| match (&*a, b) {
            (Some(t), Some(x)) => {
                if t.valor != x.valor {
                    discrepancias.push(Discrepancia {
                        campo: nombre.to_string(),
                        segun_timbre: t.valor.clone(),
                        segun_texto: x.valor,
                    });
                }
            }
            (None, Some(x)) => *a = Some(x),
            _ => {}
        };

        unir("folio", &mut base.folio, del_texto.folio);
        unir("rut_emisor", &mut base.rut_emisor, del_texto.rut_emisor);
        unir("rut_deudor", &mut base.rut_deudor, del_texto.rut_deudor);
        unir("monto_total", &mut base.monto_total, del_texto.monto_total);
        unir("fecha_emision", &mut base.fecha_emision, del_texto.fecha_emision);
        // La razón social no se contrasta: el timbre la trae truncada a 40
        // caracteres por norma del SII, así que diferir es lo esperable y
        // marcarlo sería ruido. Gana la del timbre si está.
        if base.razon_social_deudor.is_none() {
            base.razon_social_deudor = del_texto.razon_social_deudor;
        }

        base.discrepancias = discrepancias;
        base
    }
}

// ── Composición de las tres capas ─────────────────────────────────────────────
// Gated: necesita pdfium para la capa de texto y las imágenes embebidas, y
// `timbre` para el TED. Sin `timbre` el worker sigue funcionando con las otras
// dos capas; sin `pdf` no hay nada que leer.
#[cfg(all(feature = "pdf", feature = "timbre"))]
impl ExtraccionFacturaService {
    /// Lee un PDF por las tres capas y devuelve un solo resultado con la
    /// procedencia de cada campo.
    ///
    /// El OCR no se llama desde acá: vive detrás de la feature `ocr`, que
    /// arrastra librerías del sistema. El llamador decide si cae a OCR mirando
    /// si lo que volvió está vacío. Separarlo así es lo que permite iterar sobre
    /// las dos primeras capas fuera de Docker.
    pub fn extraer_de_pdf(
        &self,
        pdf_bytes: &[u8],
        documentos: &crate::aplication::service::document_manager_service::DocumentManagerService,
        timbres: &crate::aplication::service::timbre_manager_service::TimbreManagerService,
    ) -> crate::domain::models::extraccion_factura_model::ExtraccionFactura {
        let del_timbre = self
            .leer_timbre(pdf_bytes, documentos, timbres)
            .map(|l| crate::domain::models::extraccion_factura_model::ExtraccionFactura::from(&l));

        let del_texto = documentos
            .texto_de_capa(pdf_bytes)
            .ok()
            .and_then(|paginas| paginas.into_iter().next())
            .map(|p| self.desde_texto(&p))
            .unwrap_or_default();

        self.combinar(del_timbre, del_texto)
    }

    /// Busca el timbre primero en las imágenes embebidas y después en el render
    /// de la página.
    ///
    /// Ese orden no es una optimización prematura: está medido. El PDF417 suele
    /// venir como bitmap de 1 bit de ~500 px, y rasterizar la página a 3000 px lo
    /// interpola hasta destruir los bordes de los módulos — una factura que
    /// fallaba por el render decodifica nativa a la primera. Además evita buscar
    /// el símbolo en una hoja entera: 10–31 ms contra 2.100–2.600 ms.
    fn leer_timbre(
        &self,
        pdf_bytes: &[u8],
        documentos: &crate::aplication::service::document_manager_service::DocumentManagerService,
        timbres: &crate::aplication::service::timbre_manager_service::TimbreManagerService,
    ) -> Option<crate::domain::models::ted_model::LecturaTimbre> {
        if let Ok(imagenes) = documentos.imagenes_embebidas_primera_pagina(pdf_bytes) {
            // Vienen también los logos; se prueba una por una.
            if let Some(l) = imagenes.iter().find_map(|img| timbres.leer(img)) {
                return Some(l);
            }
        }

        // Timbre dibujado como miles de rectángulos en vez de como imagen. Va
        // antes del render de página porque es más barato (3–6 ms contra ~230) y
        // porque para el facturador que lo usa el render NO funciona a ninguna
        // resolución: sus módulos se dibujan más angostos que su celda, así que
        // las corridas de módulos vecinos salen separadas y PDF417 codifica
        // justamente en el ancho de las corridas.
        if let Ok(Some(simbolo)) = documentos.simbolo_vectorial_primera_pagina(pdf_bytes) {
            if let Some(l) = timbres.leer(&simbolo) {
                return Some(l);
            }
        }

        let png = documentos.render_first_page_png_from_pdf(pdf_bytes).ok()?;
        let img = image::load_from_memory(&png).ok()?;
        timbres.leer(&img)
    }
}
