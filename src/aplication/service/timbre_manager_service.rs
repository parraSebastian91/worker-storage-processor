//! Lectura del Timbre Electrónico (TED) del PDF417 que toda factura
//! electrónica chilena lleva impresa.
//!
//! Por qué existe: hoy los cuatro campos de la factura se sacan con regex sobre
//! Tesseract (`DocumentManagerService`), que pelea con el ruido del OCR y cae a
//! "el primer RUT que coincida" cuando no encuentra el del deudor. El mismo PDF
//! ya trae esos cuatro campos como bytes exactos dentro del timbre, y firmados.
//!
//! Recibe la `DynamicImage` que el worker **ya renderiza**, así que no agrega
//! una pipeline nueva: es un consumidor más de la imagen que se produce para el
//! OCR. Análisis completo en `analisis/02`, `analisis/03` y `analisis/08` del
//! repo SEIS_APP.

use crate::domain::models::ted_model::{LecturaTimbre, Ted, Verificacion};

pub struct TimbreManagerService {}

impl Default for TimbreManagerService {
    fn default() -> Self {
        Self::new()
    }
}

impl TimbreManagerService {
    pub fn new() -> Self {
        Self {}
    }

    /// Lee el timbre de una página ya renderizada.
    ///
    /// `None` = no se encontró un PDF417 legible. No es un error: hay respaldos
    /// que legítimamente no tienen timbre (guías, cotizaciones, órdenes de
    /// compra), y ahí el OCR sigue siendo la única vía.
    #[cfg(feature = "timbre")]
    pub fn leer(&self, img: &image::DynamicImage) -> Option<LecturaTimbre> {
        let ted = self.decodificar(img)?;
        Some(Self::interpretar(&ted))
    }

    #[cfg(not(feature = "timbre"))]
    pub fn leer(&self, _img: &image::DynamicImage) -> Option<LecturaTimbre> {
        None
    }

    /// Decodifica el PDF417 y devuelve el TED como texto latin-1.
    ///
    /// Prueba la imagen tal cual y, si falla, invertida. No es defensivo de
    /// más: hay facturadores que embeben el timbre como bitmap con la polaridad
    /// invertida (barras blancas sobre fondo negro). pdfium lo pinta bien en la
    /// página, pero si se lee el bitmap embebido —que es lo que conviene hacer,
    /// porque evita el reescalado— viene al revés y ningún decodificador lo
    /// toma. Medido sobre una factura real.
    #[cfg(feature = "timbre")]
    pub fn decodificar(&self, img: &image::DynamicImage) -> Option<String> {
        self.decodificar_directo(img).or_else(|| {
            let mut invertida = img.clone();
            image::imageops::invert(&mut invertida);
            self.decodificar_directo(&invertida)
        })
    }

    #[cfg(feature = "timbre")]
    fn decodificar_directo(&self, img: &image::DynamicImage) -> Option<String> {
        let (w, h) = (img.width(), img.height());
        let luma = img.to_luma8().into_raw();

        let mut hints = rxing::DecodeHints::default();
        hints.PossibleFormats = Some([rxing::BarcodeFormat::PDF_417].into_iter().collect());
        hints.TryHarder = Some(true);

        let resultado = rxing::helpers::detect_in_luma_with_hints(
            luma,
            w,
            h,
            Some(rxing::BarcodeFormat::PDF_417),
            &mut hints,
        )
        .ok()?;

        Self::bytes_del_simbolo(&resultado).map(|b| Self::latin1(&b))
    }

    /// Recupera los BYTES del símbolo, que es lo único que sirve.
    ///
    /// El TED viaja en ISO-8859-1 y su firma cubre esos bytes exactos: si los
    /// `0x80–0xFF` (acentos, `ñ`) se corrompen, el DD deja de verificar y el
    /// síntoma aparece lejísimos de la causa.
    ///
    /// `rxing` no expone bytes crudos para PDF417 —`getRawBytes()` viene vacío y
    /// no emite `BYTE_SEGMENTS`—, solo `getText()`. Ese `String` mapea **un code
    /// point por byte**, así que se reconstruyen exactos. La comprobación de que
    /// ningún code point supere `0xFF` **no es defensiva de más**: es lo que
    /// detectaría que una versión futura de `rxing` pasara a interpretar UTF-8,
    /// que si no rompería la verificación en silencio.
    #[cfg(feature = "timbre")]
    fn bytes_del_simbolo(r: &rxing::RXingResult) -> Option<Vec<u8>> {
        if let Some(rxing::RXingResultMetadataValue::ByteSegments(segs)) = r
            .getRXingResultMetadata()
            .get(&rxing::RXingResultMetadataType::BYTE_SEGMENTS)
        {
            let bytes: Vec<u8> = segs.concat();
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
        let crudos = r.getRawBytes();
        if !crudos.is_empty() {
            return Some(crudos.to_vec());
        }
        let texto = r.getText();
        if texto.chars().any(|c| (c as u32) > 0xFF) {
            return None; // el decodificador interpretó los bytes: inservible
        }
        Some(texto.chars().map(|c| c as u8).collect())
    }

    /// ISO-8859-1 → String: cada byte es un code point. Nunca `from_utf8`.
    fn latin1(b: &[u8]) -> String {
        b.iter().map(|&c| c as char).collect()
    }

    /// Parsea el TED y verifica su firma.
    pub fn interpretar(ted: &str) -> LecturaTimbre {
        let dd = Self::entre(ted, "<DD>", "</DD>");
        let (ted_parseado, dd) = match (dd.and_then(Self::parsear_dd), dd) {
            (Some(t), Some(d)) => (t, d),
            _ => {
                return LecturaTimbre {
                    ted: Ted {
                        rut_emisor: String::new(),
                        tipo_dte: 0,
                        folio: 0,
                        fecha_emision: String::new(),
                        rut_receptor: String::new(),
                        razon_social_receptor: String::new(),
                        monto_total: 0,
                        primer_item: None,
                    },
                    verificacion: Verificacion::TedIncompleto,
                }
            }
        };

        let (verificacion, dd_bueno) = Self::verificar(ted, dd);
        // Si hubo reparación, el DD bueno es el reparado: sus campos son los que
        // el facturador firmó, no los que el símbolo impreso dice. Se reparsea,
        // porque si no mostraríamos "N]2" sabiendo que dice "Nº2".
        let ted_parseado = match dd_bueno {
            Some(d) if d != dd => Self::parsear_dd(&d).unwrap_or(ted_parseado),
            _ => ted_parseado,
        };
        LecturaTimbre {
            ted: ted_parseado,
            verificacion,
        }
    }

    fn parsear_dd(dd: &str) -> Option<Ted> {
        Some(Ted {
            rut_emisor: Self::texto(Self::contenido(dd, "RE")?),
            tipo_dte: Self::contenido(dd, "TD")?.trim().parse().ok()?,
            folio: Self::contenido(dd, "F")?.trim().parse().ok()?,
            fecha_emision: Self::texto(Self::contenido(dd, "FE")?),
            rut_receptor: Self::texto(Self::contenido(dd, "RR")?),
            razon_social_receptor: Self::texto(Self::contenido(dd, "RSR")?),
            monto_total: Self::contenido(dd, "MNT")?.trim().parse().ok()?,
            primer_item: Self::contenido(dd, "IT1").map(Self::texto),
        })
    }

    /// El valor de un campo, con las entidades XML resueltas.
    ///
    /// Va DESPUÉS de verificar la firma, nunca antes: la firma cubre los bytes
    /// tal como viajan, con `&amp;` y todo. Lo que se desescapa es sólo lo que
    /// se muestra — una razón social que diga "MUELLE MELBOURNE &amp;amp; CLARK"
    /// está mal, y ese nombre termina en pantalla y en la base.
    fn texto(bruto: &str) -> String {
        bruto
            .trim()
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            // `&amp;` al final: si no, "&amp;lt;" se convertiría en "<".
            .replace("&amp;", "&")
    }

    /// Verifica el `<FRMT>` contra la clave pública del `<CAF>` embebido.
    ///
    /// Alcance: prueba que el DD no fue alterado y que lo firmó el tenedor de
    /// ese CAF. NO prueba que el SII haya autorizado ese folio a ese RUT — para
    /// eso habría que verificar la `<FRMA>` del propio CAF contra la clave del
    /// SII, que no viene en el documento.
    #[cfg(feature = "timbre")]
    fn verificar(ted: &str, dd: &str) -> (Verificacion, Option<String>) {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        use rsa::signature::Verifier;
        use rsa::{BigUint, RsaPublicKey};
        use sha1::Sha1;

        let frmt_b64: String = match Self::contenido(ted, "FRMT") {
            Some(s) => s.split_whitespace().collect(),
            None => return (Verificacion::TedIncompleto, None),
        };

        // ⚠️ El CAF embebido en el TED trae la clave como módulo y exponente en
        // <RSAPK>, NO como PEM en <RSAPUBK>: ese tag vive en el archivo CAF que
        // entrega el SII (dentro de <AUTORIZACION>), no en el timbre. Medido
        // sobre facturas reales de 4 facturadores distintos.
        let rsapk = match Self::entre(dd, "<RSAPK>", "</RSAPK>") {
            Some(s) => s,
            None => return (Verificacion::SinClavePublica, None),
        };
        let (m, e) = match (Self::contenido(rsapk, "M"), Self::contenido(rsapk, "E")) {
            (Some(m), Some(e)) => (m, e),
            _ => return (Verificacion::SinClavePublica, None),
        };
        let (m, e) = match (
            STANDARD.decode(m.trim().replace(['\n', '\r', ' '], "")),
            STANDARD.decode(e.trim().replace(['\n', '\r', ' '], "")),
        ) {
            (Ok(m), Ok(e)) => (m, e),
            _ => return (Verificacion::SinClavePublica, None),
        };

        // `new_unchecked` y no `new`: los CAF del SII usan claves de 512 bits
        // con exponente 3, que el crate rechaza por debajo de sus mínimos
        // modernos. No las elegimos nosotros — vienen así en el documento.
        let clave = RsaPublicKey::new_unchecked(
            BigUint::from_bytes_be(&m),
            BigUint::from_bytes_be(&e),
        );

        let firma = match STANDARD.decode(frmt_b64.trim()) {
            Ok(f) => f,
            Err(_) => return (Verificacion::FirmaInvalida, None),
        };
        let firma = match Signature::try_from(firma.as_slice()) {
            Ok(f) => f,
            Err(_) => return (Verificacion::FirmaInvalida, None),
        };

        // Se prueban las DOS formas del DD, y el orden importa.
        //
        // Lo que se firmó es el DD tal como lo armó el facturador. Medido sobre
        // facturas reales de 4 facturadores distintos, el DD viaja **ya
        // compacto**: cero whitespace entre tags, y verifica tal cual. Pero la
        // especificación no obliga a eso, y un emisor podría mandarlo con
        // saltos de línea; ahí hay dos posibilidades —que haya firmado el
        // compacto o el que viaja— y no se puede saber de antemano cuál.
        //
        // Por eso: primero tal como viene (que es lo correcto por definición),
        // y si falla, compactado. Compactar SIEMPRE sería un error: rompería el
        // caso de un DD con whitespace firmado tal cual.
        let verificador: VerifyingKey<Sha1> = VerifyingKey::new(clave);
        let a_bytes = |t: &str| -> Vec<u8> { t.chars().map(|c| c as u8).collect() };

        if verificador.verify(&a_bytes(dd), &firma).is_ok() {
            return (Verificacion::Verificado, Some(dd.to_string()));
        }
        let compacto = Self::compactar(dd);
        if compacto != dd && verificador.verify(&a_bytes(&compacto), &firma).is_ok() {
            return (Verificacion::Verificado, Some(compacto));
        }

        // Último intento: reparar un carácter corrupto del símbolo.
        for base in [dd, compacto.as_str()] {
            if let Some(reparado) = Self::reparar(base, &|t: &str| {
                verificador.verify(&a_bytes(t), &firma).is_ok()
            }) {
                return (Verificacion::VerificadoConReparacion, Some(reparado));
            }
        }
        (Verificacion::FirmaInvalida, None)
    }

    #[cfg(not(feature = "timbre"))]
    fn verificar(_ted: &str, _dd: &str) -> (Verificacion, Option<String>) {
        (Verificacion::SinClavePublica, None)
    }

    /// Busca el DD original cuando el símbolo impreso no coincide con lo firmado.
    ///
    /// Por qué hace falta: hay facturadores cuyo generador de timbre y cuyo
    /// firmador no producen exactamente el mismo texto. Medido sobre 109
    /// facturas reales, dos familias de defecto:
    ///
    /// 1. **Carácter fuera del ASCII imprimible.** El modo texto de PDF417 no lo
    ///    representa, y un encoder que no cambia a modo byte emite en su lugar
    ///    uno de la tabla de puntuación: se midió un `º` (0xBA) impreso como `]`
    ///    (0x5D).
    /// 2. **Escapado XML inconsistente.** Se midió un DD firmado con `&quot;` e
    ///    impreso con `"` literal, en el mismo documento donde el `&amp;` sí
    ///    viajó escapado.
    ///
    /// Esos timbres no verifican tal cual **para nadie**, y acusarlos de firma
    /// inválida sería acusar de adulterada a una factura legítima.
    ///
    /// Por qué es seguro: no se adivina nada, se **busca** el texto cuyo SHA1
    /// cuadra con el que la firma declara. Que cuadre es prueba, no indicio: no
    /// se puede acertar un SHA1 por casualidad, y nadie puede usar esto para
    /// hacer verificar un DD adulterado, porque eso exigiría una colisión. Lo
    /// peor que puede pasar es gastar unos microsegundos y no encontrar nada.
    #[cfg(feature = "timbre")]
    fn reparar(dd: &str, verifica: &dyn Fn(&str) -> bool) -> Option<String> {
        // Variantes de escapado: se prueban todas las combinaciones de estas
        // cuatro sustituciones globales, incluida la vacía.
        const ESCAPES: [(&str, &str); 4] = [
            ("\"", "&quot;"),
            ("&quot;", "\""),
            ("'", "&apos;"),
            ("&amp;", "&"),
        ];

        for mascara in 0u8..(1 << ESCAPES.len()) {
            let mut variante = dd.to_string();
            for (i, (de, a)) in ESCAPES.iter().enumerate() {
                if mascara & (1 << i) != 0 {
                    variante = Self::sustituir_en_contenido(&variante, de, a);
                }
            }
            if mascara != 0 && verifica(&variante) {
                return Some(variante);
            }
            if let Some(r) = Self::reparar_un_caracter(&variante, verifica) {
                return Some(r);
            }
        }
        None
    }

    /// Sustituye sólo dentro del contenido de los elementos, nunca dentro de una
    /// etiqueta.
    ///
    /// Necesario: el DD trae `version="1.0"` y `algoritmo="SHA1withRSA"`, y un
    /// `replace` a secas de `"` por `&quot;` rompería esas comillas de atributo,
    /// que son marcación y no texto. El defecto que se quiere reparar vive en el
    /// valor de un elemento —se midió un `3"` de pulgadas firmado como
    /// `3&quot;`—, nunca en la marcación.
    #[cfg(feature = "timbre")]
    fn sustituir_en_contenido(xml: &str, de: &str, a: &str) -> String {
        let mut salida = String::with_capacity(xml.len());
        let mut resto = xml;
        while let Some(i) = resto.find('<') {
            let (contenido, desde_tag) = resto.split_at(i);
            salida.push_str(&contenido.replace(de, a));
            match desde_tag.find('>') {
                Some(j) => {
                    salida.push_str(&desde_tag[..=j]);
                    resto = &desde_tag[j + 1..];
                }
                None => {
                    salida.push_str(desde_tag);
                    return salida;
                }
            }
        }
        salida.push_str(&resto.replace(de, a));
        salida
    }

    /// Sustituye UN carácter sospechoso por cada byte alto de Latin-1.
    ///
    /// Acotado a propósito: sólo posiciones cuyo carácter pertenece a la tabla
    /// de puntuación de PDF417 —los únicos que el encoder puede haber emitido
    /// por este motivo—, y nunca más de `MAX_POSICIONES`. Sin ese tope, un DD
    /// con muchos corchetes legítimos (`[BE]`, `[RT]` en descripciones de
    /// remedios: 29 apariciones en el corpus) haría crecer el trabajo sin
    /// aportar nada.
    #[cfg(feature = "timbre")]
    fn reparar_un_caracter(dd: &str, verifica: &dyn Fn(&str) -> bool) -> Option<String> {
        /// Caracteres de la tabla de puntuación de PDF417 que **no tienen nada
        /// que hacer dentro de un valor del TED**. El conjunto es angosto: la
        /// tabla completa incluye `<`, `>`, `/` y comillas, que son la marcación
        /// XML del propio DD, y meterlos haría que cada DD tuviera cientos de
        /// posiciones candidatas.
        const SOSPECHOSOS: &str = "[]\\_@;~`|{}^";
        const MAX_POSICIONES: usize = 4;

        let chars: Vec<char> = dd.chars().collect();
        let sospechosas: Vec<usize> = chars
            .iter()
            .enumerate()
            .filter(|(_, c)| SOSPECHOSOS.contains(**c))
            .map(|(i, _)| i)
            .collect();
        if sospechosas.is_empty() || sospechosas.len() > MAX_POSICIONES {
            return None;
        }

        // 0xA0–0xFF: el rango de Latin-1 con los caracteres que un facturador
        // chileno puede poner en una razón social (º, °, ª, Ñ, acentos).
        for &i in &sospechosas {
            for byte in 0xA0u32..=0xFF {
                let mut intento = chars.clone();
                intento[i] = char::from_u32(byte).unwrap();
                let texto: String = intento.into_iter().collect();
                if verifica(&texto) {
                    return Some(texto);
                }
            }
        }
        None
    }

    /// Quita el whitespace entre tags. Equivale a `>\s+<` → `><`.
    ///
    /// Itera por CHARS y no por bytes: en un `String` de Rust una `Ñ` son dos
    /// bytes UTF-8, y recorrer `as_bytes()` empujándolos como `char` los parte
    /// en dos caracteres distintos. El texto sigue pareciendo correcto en ASCII
    /// y la firma deja de verificar solo cuando hay acentos — que es la mitad
    /// de los casos y la que no aparece en una prueba rápida.
    fn compactar(dd: &str) -> String {
        let chars: Vec<char> = dd.chars().collect();
        let mut salida = String::with_capacity(dd.len());
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            salida.push(c);
            if c == '>' {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && chars[j] == '<' {
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        salida
    }

    /// Subcadena entre dos marcas, incluyéndolas.
    fn entre<'a>(s: &'a str, abre: &str, cierra: &str) -> Option<&'a str> {
        let i = s.find(abre)?;
        let j = s[i..].find(cierra)? + i + cierra.len();
        Some(&s[i..j])
    }

    /// Contenido de un tag, sin las tags. Tolera atributos (`<CAF version=…>`).
    fn contenido<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
        let mut desde = 0;
        loop {
            let i = s[desde..].find(&format!("<{}", tag))? + desde;
            let despues = i + tag.len() + 1;
            let siguiente = *s.as_bytes().get(despues)? as char;
            // `<F>` no debe matchear `<FE>`: el char siguiente al nombre tiene
            // que cerrar la tag o abrir un atributo.
            if siguiente != '>' && siguiente != ' ' {
                desde = i + 1;
                continue;
            }
            let apertura = s[i..].find('>')? + i + 1;
            let cierre = s[apertura..].find(&format!("</{}>", tag))? + apertura;
            return Some(&s[apertura..cierre]);
        }
    }
}
