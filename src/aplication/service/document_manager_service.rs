#[cfg(feature = "pdf")]
use std::io::Cursor;

#[cfg(feature = "pdf")]
use image::ImageFormat;
#[cfg(feature = "pdf")]
use image::DynamicImage;
use image::{GrayImage, Luma};
#[cfg(feature = "pdf")]
use pdfium_render::prelude::{PdfPageObjectsCommon, PdfRenderConfig, Pdfium};
use regex::Regex;
#[cfg(feature = "ocr")]
use tesseract::Tesseract;

use crate::domain::errors::media_error::MediaError;
#[cfg(feature = "ocr")]
use tracing::error;
#[cfg(feature = "pdf")]
use tracing::{debug, info};
use crate::domain::models::factura_data_model::InvoiceData;

pub struct DocumentManagerService {}

impl DocumentManagerService {
    pub fn new() -> Self {
        Self {}
    }

    pub fn render_first_page_png_from_pdf(&self, pdf_bytes: &[u8]) -> Result<Vec<u8>, MediaError> {
        #[cfg(not(feature = "pdf"))]
        {
            let _ = pdf_bytes;
            return Err(MediaError::PdfRenderError(
                "Lectura de PDF no habilitada. Compila con --features pdf".to_string(),
            ));
        }

        #[cfg(feature = "pdf")]
        {
            let page_image = self.render_first_page_image(pdf_bytes)?;

            let mut png_cursor = Cursor::new(Vec::new());
            page_image
                .write_to(&mut png_cursor, ImageFormat::Png)
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

            Ok(png_cursor.into_inner())
        }
    }

    #[cfg(feature = "pdf")]
    fn render_first_page_image(&self, pdf_bytes: &[u8]) -> Result<DynamicImage, MediaError> {
        let pdfium = Self::pdfium()?;

        let document = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

        let first_page = document
            .pages()
            .iter()
            .next()
            .ok_or_else(|| MediaError::PdfRenderError("El PDF no contiene páginas".to_string()))?;

        first_page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(3000)
                    .render_form_data(true),
            )
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))
            .map(|bitmap| bitmap.as_image())
    }

    /// Texto **ya incorporado** en el PDF, página por página.
    ///
    /// Una factura electrónica generada por software trae sus campos acá, en
    /// bytes exactos: no hay que adivinarlos con OCR. Medido sobre las 6
    /// facturas reales, las 6 traen capa de texto y leerla cuesta decenas de
    /// milisegundos contra los segundos que cuesta rasterizar y pasar Tesseract
    /// (ver `docs/general/findings/cotizadores-externos/`).
    ///
    /// Devuelve una entrada por página en vez de un solo String a propósito: un
    /// respaldo puede traer la factura y su orden de compra en el mismo archivo,
    /// y mezclar las dos páginas haría que una regex se lleve el monto del
    /// documento equivocado. Quién decide qué páginas mirar es el llamador.
    ///
    /// Un PDF escaneado devuelve páginas vacías o casi: eso **no es un error**,
    /// es la señal de que hay que caer al OCR.
    #[cfg(feature = "pdf")]
    pub fn texto_de_capa(&self, pdf_bytes: &[u8]) -> Result<Vec<String>, MediaError> {
        let pdfium = Self::pdfium()?;
        let documento = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

        let mut paginas = Vec::new();
        for pagina in documento.pages().iter() {
            let texto = pagina
                .text()
                .map(|t| t.all())
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
            paginas.push(texto);
        }
        info!("Capa de texto: {} página(s) leídas", paginas.len());
        Ok(paginas)
    }

    /// Imágenes embebidas de la primera página, **a resolución nativa**.
    ///
    /// Para leer el timbre esto importa más de lo que parece. El PDF417 suele
    /// venir como un bitmap de 1 bit de unos 500 px de ancho; al rasterizar la
    /// página a 3000 px ese bitmap se interpola a ~1285 px y el suavizado
    /// destruye los bordes de los módulos. Leyéndolo nativo no hay interpolación
    /// y además no hay que buscar el símbolo en una hoja entera: medido, 10–31 ms
    /// contra 2.100–2.600 ms del render de página (`analisis/08` §11).
    ///
    /// Vienen también los logos; el llamador prueba una por una y se queda con
    /// la que decodifique.
    #[cfg(feature = "pdf")]
    pub fn imagenes_embebidas_primera_pagina(
        &self,
        pdf_bytes: &[u8],
    ) -> Result<Vec<DynamicImage>, MediaError> {
        let pdfium = Self::pdfium()?;
        let documento = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

        let pagina = documento
            .pages()
            .iter()
            .next()
            .ok_or_else(|| MediaError::PdfRenderError("El PDF no contiene páginas".to_string()))?;

        // `get_raw_image` da el bitmap tal como está almacenado, sin aplicar la
        // transformación de la página. Es justo lo que se quiere: cualquier
        // escalado lo hace la página, y es lo que hay que evitar.
        let mut imagenes = Vec::new();
        Self::recolectar_imagenes(pagina.objects(), &mut imagenes);
        info!("Imágenes embebidas en la primera página: {}", imagenes.len());
        Ok(imagenes)
    }

    /// Recorre los objetos bajando a los Form XObject.
    ///
    /// `page.objects()` sólo devuelve los de primer nivel, y medido sobre 109
    /// facturas reales eso deja el timbre fuera de alcance en **67 de 93**: esos
    /// PDF muestran una sola imagen suelta —el logo— y llevan el PDF417 adentro
    /// de un form. Sin bajar, esas 67 caían al render de página completa, que
    /// cuesta ~232 ms contra los ~13 ms de leer el bitmap embebido.
    ///
    /// La profundidad va acotada porque un PDF puede anidar forms sin fondo, y
    /// un documento hostil podría hacerlo a propósito.
    #[cfg(feature = "pdf")]
    fn recolectar_imagenes(
        objetos: &pdfium_render::prelude::PdfPageObjects,
        salida: &mut Vec<DynamicImage>,
    ) {
        for objeto in objetos.iter() {
            Self::recolectar_de_objeto(&objeto, 0, salida);
        }
    }

    #[cfg(feature = "pdf")]
    fn recolectar_de_objeto(
        objeto: &pdfium_render::prelude::PdfPageObject,
        profundidad: usize,
        salida: &mut Vec<DynamicImage>,
    ) {
        /// Un PDF puede anidar forms sin fondo, y uno hostil podría hacerlo a
        /// propósito. Medido sobre el corpus, el timbre nunca pasa del primer
        /// nivel; cuatro deja margen de sobra.
        const MAX_PROFUNDIDAD: usize = 4;

        if let Some(img) = objeto.as_image_object() {
            if let Ok(d) = img.get_raw_image() {
                salida.push(d);
            }
            return;
        }
        if profundidad >= MAX_PROFUNDIDAD {
            return;
        }
        if let Some(form) = objeto.as_x_object_form_object() {
            for i in form.as_range() {
                if let Ok(hijo) = form.get(i) {
                    Self::recolectar_de_objeto(&hijo, profundidad + 1, salida);
                }
            }
        }
    }

    /// Reconstruye el PDF417 cuando viene dibujado como miles de rectángulos en
    /// vez de como una imagen.
    ///
    /// Hay un facturador —15% del corpus de 109 facturas reales— que dibuja el
    /// timbre con ~19.000 paths. Rasterizar la página no sirve a ninguna
    /// resolución (medido: 0/6 a 3000, 5000, 7000 y 9000 px) ni recortando la
    /// región (0/3), porque el problema no es el muestreo sino la **geometría**:
    /// las filas del símbolo son tan bajas respecto del ancho de módulo que la
    /// imagen se lee como rayado vertical y el detector no encuentra las filas.
    ///
    /// Acá no se decodifica nada: se usan las coordenadas —que son exactas, sin
    /// aliasing— para armar la matriz de módulos y repintarla con una proporción
    /// sana. ZXing hace el resto (muestreo, codewords, Reed-Solomon sobre
    /// GF(929)); reimplementar eso sería rehacer lo que ya está resuelto aguas
    /// abajo del problema.
    #[cfg(feature = "pdf")]
    pub fn simbolo_vectorial_primera_pagina(
        &self,
        pdf_bytes: &[u8],
    ) -> Result<Option<DynamicImage>, MediaError> {
        use pdfium_render::prelude::PdfPageObjectCommon;

        let pdfium = Self::pdfium()?;
        let documento = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
        let pagina = documento
            .pages()
            .iter()
            .next()
            .ok_or_else(|| MediaError::PdfRenderError("El PDF no contiene páginas".to_string()))?;

        // Rectángulos candidatos: los paths chicos. Los bordes de tabla y las
        // líneas del formulario son largos, y quedan fuera por tamaño.
        let mut cajas: Vec<[f32; 4]> = Vec::new();
        for objeto in pagina.objects().iter() {
            if objeto.as_path_object().is_none() {
                continue;
            }
            let Ok(b) = objeto.bounds() else { continue };
            let (w, h) = (b.width().value, b.height().value);
            if w > 0.0 && h > 0.0 && w < 20.0 && h < 20.0 {
                cajas.push([b.left().value, b.bottom().value, b.right().value, b.top().value]);
            }
        }

        // Un timbre son miles de módulos. Con menos, lo que haya es ruido del
        // formulario y no vale la pena seguir.
        const MIN_RECTANGULOS: usize = 500;
        if cajas.len() < MIN_RECTANGULOS {
            return Ok(None);
        }

        Ok(Self::matriz_a_imagen(&cajas))
    }

    /// Las cajas crudas de los rectángulos chicos de la primera página.
    /// Diagnóstico del laboratorio: sirve para ver la geometría real antes de
    /// decidir cómo se agrupa en módulos.
    #[cfg(feature = "pdf")]
    pub fn rectangulos_primera_pagina(&self, pdf_bytes: &[u8]) -> Result<Vec<[f32; 4]>, MediaError> {
        use pdfium_render::prelude::PdfPageObjectCommon;
        let pdfium = Self::pdfium()?;
        let documento = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
        let pagina = documento
            .pages()
            .iter()
            .next()
            .ok_or_else(|| MediaError::PdfRenderError("El PDF no contiene páginas".to_string()))?;
        let mut cajas = Vec::new();
        for objeto in pagina.objects().iter() {
            if objeto.as_path_object().is_none() {
                continue;
            }
            let Ok(b) = objeto.bounds() else { continue };
            let (w, h) = (b.width().value, b.height().value);
            if w > 0.0 && h > 0.0 && w < 20.0 && h < 20.0 {
                cajas.push([b.left().value, b.bottom().value, b.right().value, b.top().value]);
            }
        }
        Ok(cajas)
    }

    /// Pasa de rectángulos en puntos PDF a un bitmap de módulos.
    ///
    /// La medida que manda es el **paso** entre posiciones vecinas, no el tamaño
    /// del rectángulo. Medido sobre los dos facturadores que dibujan el timbre:
    /// uno usa módulos de 0,374 pt de ancho ubicados cada 0,5025, o sea que cada
    /// módulo se dibuja **más angosto que su celda**. Eso deja una ranura blanca
    /// entre módulos vecinos, así que una corrida de tres módulos oscuros no se
    /// imprime como una barra ancha sino como tres barras finas separadas — y
    /// PDF417 codifica precisamente en el ancho de las corridas.
    ///
    /// Por eso el símbolo impreso no decodifica a ninguna resolución: no es un
    /// problema de muestreo, el dibujo está mal. Las coordenadas, en cambio,
    /// dicen exactamente en qué celda va cada módulo.
    #[cfg(feature = "pdf")]
    fn matriz_a_imagen(cajas: &[[f32; 4]]) -> Option<DynamicImage> {
        use image::{GrayImage, Luma};

        // 1. Quedarse con los rectángulos del tamaño dominante. En la página hay
        //    también líneas de formulario: medido, un PDF con 29.968 rectángulos
        //    chicos trae además algunos de 13,25 pt de alto, que no son módulos.
        let modal = |v: Vec<f32>| -> Option<f32> {
            let mut conteo: Vec<(f32, usize)> = Vec::new();
            for x in v {
                match conteo.iter_mut().find(|(k, _)| (*k - x).abs() < 0.01) {
                    Some((_, n)) => *n += 1,
                    None => conteo.push((x, 1)),
                }
            }
            conteo.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k)
        };
        let w = modal(cajas.iter().map(|c| c[2] - c[0]).collect())?;
        let h = modal(cajas.iter().map(|c| c[3] - c[1]).collect())?;
        let modulos: Vec<[f32; 4]> = cajas
            .iter()
            .filter(|c| (c[2] - c[0] - w).abs() < w * 0.3 && (c[3] - c[1] - h).abs() < h * 0.3)
            .copied()
            .collect();
        if modulos.len() < 500 {
            return None;
        }

        // 2. El símbolo es el grupo denso. Rectángulos sueltos en otra parte de
        //    la hoja estirarían el bounding box y la grilla saldría cualquier
        //    cosa: medido, un salto de 361 pt entre dos filas.
        let grupo = |vals: &[f32], salto_max: f32| -> (f32, f32) {
            let mut v: Vec<f32> = vals.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let (mut mejor, mut ini) = ((v[0], v[0], 0usize), 0usize);
            for i in 1..=v.len() {
                if i == v.len() || v[i] - v[i - 1] > salto_max {
                    if i - ini > mejor.2 {
                        mejor = (v[ini], v[i - 1], i - ini);
                    }
                    ini = i;
                }
            }
            (mejor.0, mejor.1)
        };
        let (gx0, gx1) = grupo(&modulos.iter().map(|c| c[0]).collect::<Vec<_>>(), w * 12.0);
        let (gy0, gy1) = grupo(&modulos.iter().map(|c| c[1]).collect::<Vec<_>>(), h * 12.0);
        let modulos: Vec<[f32; 4]> = modulos
            .into_iter()
            .filter(|c| c[0] >= gx0 - w && c[0] <= gx1 + w && c[1] >= gy0 - h && c[1] <= gy1 + h)
            .collect();
        if modulos.len() < 500 {
            debug!(modulos = modulos.len(), "Tras agrupar no quedan módulos suficientes");
            return None;
        }

        // 3. El paso es la distancia MÁS FRECUENTE entre posiciones vecinas, no
        //    la mínima. Medido: hay facturadores cuyas coordenadas traen jitter
        //    de 0,01 pt, y tomar el mínimo daría un paso cien veces menor que el
        //    módulo real.
        let paso = |vals: &[f32]| -> Option<f32> {
            let mut v: Vec<f32> = vals.to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.dedup_by(|a, b| (*a - *b).abs() < 0.05);
            modal(v.windows(2).map(|w| w[1] - w[0]).collect())
        };
        let paso_x = paso(&modulos.iter().map(|c| c[0]).collect::<Vec<_>>())?;
        let paso_y = paso(&modulos.iter().map(|c| c[1]).collect::<Vec<_>>())?;
        if paso_x <= 0.0 || paso_y <= 0.0 {
            return None;
        }

        let x0 = modulos.iter().map(|c| c[0]).fold(f32::INFINITY, f32::min);
        let y1 = modulos.iter().map(|c| c[3]).fold(f32::NEG_INFINITY, f32::max);
        let celda = |v: f32, paso: f32| (v / paso).round() as i64;
        // La fila va por PISO y no por redondeo: si el facturador subdivide cada
        // módulo en dos cuadrados apilados, el de abajo cae a media celda y el
        // redondeo lo mandaría a la fila siguiente, partiendo cada fila en dos.
        let fila_de = |v: f32, paso: f32| (v / paso + 1e-3).floor() as i64;
        let columnas = modulos.iter().map(|c| celda(c[0] - x0, paso_x)).max()? + 1;
        let filas = modulos.iter().map(|c| fila_de(y1 - c[3], paso_y)).max()? + 1;

        // PDF417 admite de 3 a 90 filas, y una fila mide 17·(columnas+4)+1
        // módulos, así que menos de 17 de ancho no es un símbolo.
        // Una fila de PDF417 mide SIEMPRE 17·k+1 módulos, y el símbolo tiene a
        // lo sumo 90 filas. Esas dos invariantes alcanzan para corregir el caso
        // en que el facturador dibuja cada módulo subdividido: medido, uno pinta
        // cada módulo como dos cuadrados apilados, así que el paso vertical
        // observado es la mitad del alto real de fila y salían 172 filas donde
        // hay 86.
        let factor_y = ((filas as f32) / 90.0).ceil().max(1.0) as i64;
        let (paso_y, filas) = (paso_y * factor_y as f32, (filas + factor_y - 1) / factor_y);

        debug!(
            modulos = modulos.len(),
            paso_x, paso_y, columnas, filas, factor_y,
            "Geometría del timbre vectorial"
        );
        if !(3..=90).contains(&filas) || !(17..=4000).contains(&columnas) {
            return None;
        }
        // Si el ancho no es 17·k+1, lo que se agrupó no es un PDF417 y seguir
        // sólo gastaría tiempo del decodificador.
        if (columnas - 1) % 17 != 0 {
            debug!(columnas, "El ancho no es 17·k+1: lo agrupado no es un PDF417");
            return None;
        }
        let (columnas, filas) = (columnas as usize, filas as usize);

        let mut matriz = vec![vec![false; columnas]; filas];
        for c in &modulos {
            let col = celda(c[0] - x0, paso_x) as usize;
            // Cuántas celdas cubre: 1 si hay un rectángulo por módulo, N si el
            // facturador fusionó la corrida en uno solo.
            let ancho = ((((c[2] - c[0]) / paso_x).round()) as usize).max(1);
            let fil = (fila_de(y1 - c[3], paso_y) as usize).min(filas - 1);
            let alto = ((((c[3] - c[1]) / paso_y).round()) as usize).max(1);
            for r in fil..(fil + alto).min(filas) {
                for k in col..(col + ancho).min(columnas) {
                    matriz[r][k] = true;
                }
            }
        }

        // 4. Repintado con proporción sana y sin ranuras: los módulos vecinos se
        //    tocan, que es lo que el símbolo original no hace. El margen (quiet
        //    zone) es obligatorio para que el detector encuentre los patrones de
        //    inicio y fin.
        const PX_MODULO: u32 = 3;
        const PX_FILA: u32 = 9;
        const MARGEN: u32 = PX_MODULO * 4;

        let ancho_img = columnas as u32 * PX_MODULO + MARGEN * 2;
        let alto_img = filas as u32 * PX_FILA + MARGEN * 2;
        let mut img = GrayImage::from_pixel(ancho_img, alto_img, Luma([255u8]));
        for (r, fila) in matriz.iter().enumerate() {
            for (c, &negro) in fila.iter().enumerate() {
                if !negro {
                    continue;
                }
                for dy in 0..PX_FILA {
                    for dx in 0..PX_MODULO {
                        img.put_pixel(
                            MARGEN + c as u32 * PX_MODULO + dx,
                            MARGEN + r as u32 * PX_FILA + dy,
                            Luma([0u8]),
                        );
                    }
                }
            }
        }
        info!("Timbre vectorial reconstruido: {columnas} módulos × {filas} filas");
        Some(DynamicImage::ImageLuma8(img))
    }

    /// Cuántos objetos de cada clase tiene la primera página: (paths, imágenes,
    /// textos).
    ///
    /// Diagnóstico, no pipeline. Sirve para una sola pregunta, que es la que
    /// decide si vale la pena implementar el decodificador vectorial: cuando el
    /// timbre no se lee, ¿es porque el PDF está escaneado (sin texto), o porque
    /// el PDF417 está dibujado como miles de rectángulos en vez de como una
    /// imagen? Son dos problemas distintos con dos soluciones distintas, y a
    /// ojo no se distinguen.
    #[cfg(feature = "pdf")]
    pub fn conteo_objetos_primera_pagina(
        &self,
        pdf_bytes: &[u8],
    ) -> Result<(usize, usize, usize), MediaError> {
        let pdfium = Self::pdfium()?;
        let documento = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
        let pagina = documento
            .pages()
            .iter()
            .next()
            .ok_or_else(|| MediaError::PdfRenderError("El PDF no contiene páginas".to_string()))?;

        let (mut paths, mut imagenes, mut textos) = (0, 0, 0);
        for o in pagina.objects().iter() {
            match o.object_type() {
                pdfium_render::prelude::PdfPageObjectType::Path => paths += 1,
                pdfium_render::prelude::PdfPageObjectType::Image => imagenes += 1,
                pdfium_render::prelude::PdfPageObjectType::Text => textos += 1,
                _ => {}
            }
        }
        Ok((paths, imagenes, textos))
    }

    #[cfg(feature = "pdf")]
    fn pdfium() -> Result<Pdfium, MediaError> {
        let bindings = match std::env::var("PDFIUM_PATH") {
            Ok(ruta) if !ruta.is_empty() => Pdfium::bind_to_library(&ruta)
                .map_err(|e| MediaError::PdfRenderError(format!("PDFIUM_PATH={ruta}: {e}")))?,
            _ => Pdfium::bind_to_system_library()
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?,
        };
        Ok(Pdfium::new(bindings))
    }

    pub fn extract_text_from_pdf(
        &self,
        pdf_bytes: &[u8],
        language: &str,
    ) -> Result<String, MediaError> {
        #[cfg(not(feature = "ocr"))]
        {
            let _ = pdf_bytes;
            let _ = language;
            return Err(MediaError::OCRError(
                "OCR no habilitado. Compila con --features ocr y asegura Tesseract/Leptonica instalados"
                    .to_string(),
            ));
        }

        #[cfg(feature = "ocr")]
        {
            info!(
                "Extrayendo texto de PDF usando OCR - language: {}",
                language
            );
            info!("Cargando PDF en memoria ({} bytes)", pdf_bytes.len());
            let page_image = self.render_first_page_image(pdf_bytes)?;
            self.extract_text_from_image(page_image, language)
        }
    }

    pub fn extract_invoice_data_from_image_bytes(
        &self,
        image_bytes: &[u8],
        language: &str,
    ) -> Result<InvoiceData, MediaError> {
        #[cfg(not(feature = "ocr"))]
        {
            let _ = image_bytes;
            let _ = language;
            return Err(MediaError::OCRError(
                "OCR no habilitado. Compila con --features ocr".to_string(),
            ));
        }

        #[cfg(feature = "ocr")]
        {
            let page_image = image::load_from_memory(image_bytes)
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
            self.extract_invoice_data_from_image(page_image, language)
        }
    }

    /// Extrae datos estructurados de una factura DTE chilena
    pub fn extract_invoice_data_from_pdf(
        &self,
        pdf_bytes: &[u8],
        language: &str,
    ) -> Result<InvoiceData, MediaError> {
        #[cfg(not(feature = "ocr"))]
        {
            let _ = pdf_bytes;
            let _ = language;
            return Err(MediaError::OCRError(
                "OCR no habilitado. Compila con --features ocr".to_string(),
            ));
        }

        #[cfg(feature = "ocr")]
        {
            let page_image = self.render_first_page_image(pdf_bytes)?;
            self.extract_invoice_data_from_image(page_image, language)
        }
    }

    #[cfg(feature = "ocr")]
    fn extract_invoice_data_from_image(
        &self,
        page_image: DynamicImage,
        language: &str,
    ) -> Result<InvoiceData, MediaError> {
        // Primero extraer el texto completo desde la imagen ya renderizada
        let full_text = self.extract_text_from_image(page_image, language)?;

        // Normalizar el texto OCR en lineas individuales
        let lines = Self::normalize_ocr_lines(&full_text);

        info!("Extrayendo campos especificos de factura DTE chilena");
        info!("Lineas OCR normalizadas: {} lineas", lines.len());

        // Reconvertir a string para busquedas regex (mantiene estructura en memoria)
        let normalized_text = lines.join("\n");

        // Extraer cada campo del texto normalizado
        let numero_factura = Self::extract_numero_factura(&normalized_text);
        let rut_deudor = Self::extract_rut_deudor(&normalized_text);
        let nombre_deudor = Self::extract_nombre_deudor(&normalized_text);
        let monto_total = Self::extract_monto_total(&normalized_text);

        info!(
            "Factura: {:?}, RUT: {:?}, Deudor: {:?}, Monto: {:?}",
            numero_factura, rut_deudor, nombre_deudor, monto_total
        );

        Ok(InvoiceData {
            numero_factura,
            rut_deudor,
            nombre_deudor,
            monto_total,
            full_text: vec![],
        })
    }

    #[cfg(feature = "ocr")]
    fn extract_text_from_image(
        &self,
        page_image: DynamicImage,
        language: &str,
    ) -> Result<String, MediaError> {
        use image::GenericImageView;

        let numero_regex = Regex::new(r"(?i)\bN(?:[º°o]|o)?\s*([0-9]{1,8})\b")
            .map_err(|e| MediaError::OCRError(e.to_string()))?;
        let mut full_text = String::new();

        let i: usize = 0;
        info!("Procesando página {}/{}", i + 1, 1);

        let (img_w, img_h) = page_image.dimensions();
        let tile_rows: u32 = 4;
        let base_tile_h = img_h / tile_rows;
        let extra_h: u32 = 80; // incremento solicitado por franja
        let mut y: u32 = 0;

        for row in 0..tile_rows {
            let remaining_h = img_h.saturating_sub(y);
            if remaining_h == 0 {
                break;
            }

            let h = if row == tile_rows - 1 {
                remaining_h // la última toma solo lo que queda
            } else {
                (base_tile_h + extra_h).min(remaining_h)
            };

            let tile = page_image.crop_imm(0, y, img_w, h);

            let gray = tile.grayscale().to_luma8();
            let bw = Self::to_binary(gray.clone(), 150); // OCR general
            let bw_thin = Self::to_binary(gray, 130); // líneas más finas para tokens tipo N°/Nº

            let mut bw_cursor = Cursor::new(Vec::new());
            DynamicImage::ImageLuma8(bw)
                .write_to(&mut bw_cursor, ImageFormat::Png)
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

            let mut bw_thin_cursor = Cursor::new(Vec::new());
            DynamicImage::ImageLuma8(bw_thin)
                .write_to(&mut bw_thin_cursor, ImageFormat::Png)
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

            let tile_text = match Tesseract::new(None, Some(language))
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable("user_defined_dpi", "300")
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable("preserve_interword_spaces", "1")
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable(
                    "tessedit_char_whitelist",
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789Nnº°.-:/,()@ ",
                )
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_image_from_mem(bw_cursor.get_ref())
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .recognize()
            {
                Ok(mut tess) => match tess.get_text() {
                    Ok(text) => text,
                    Err(e) => {
                        error!("Error obteniendo texto OCR general: {}", e);
                        let err_str = e.to_string();
                        if err_str.contains("too small") || err_str.contains("cannot be recognized") {
                            info!("Tile {}/{} página {}: Imagen demasiado pequeña o no reconocible, saltando", row + 1, tile_rows, i + 1);
                            y += h;
                            continue;
                        }
                        return Err(MediaError::OCRError(err_str));
                    }
                },
                Err(e) => {
                    error!("Error obteniendo texto OCR general: {}", e);
                    let err_str = e.to_string();
                    if err_str.contains("too small") || err_str.contains("cannot be recognized") {
                        info!("Tile {}/{} página {}: Tesseract error (imagen pequeña), saltando", row + 1, tile_rows, i + 1);
                        y += h;
                        continue;
                    }
                    return Err(MediaError::OCRError(err_str));
                }
            };

            let tile_numero_text = match Tesseract::new(None, Some(language))
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable("user_defined_dpi", "300")
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable("preserve_interword_spaces", "1")
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable("tessedit_pageseg_mode", "7")
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_variable("tessedit_char_whitelist", "Nnº°oO0123456789")
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_image_from_mem(bw_thin_cursor.get_ref())
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .recognize()
            {
                Ok(mut tess) => match tess.get_text() {
                    Ok(text) => text,
                    Err(e) => {
                        error!("Error obteniendo texto OCR focalizado: {}", e);
                        let err_str = e.to_string();
                        if err_str.contains("too small") || err_str.contains("cannot be recognized") {
                            info!("Tile numero {}/{} página {}: Imagen demasiado pequeña o no reconocible, saltando", row + 1, tile_rows, i + 1);
                            y += h;
                            continue;
                        }
                        return Err(MediaError::OCRError(err_str));
                    }
                },
                Err(e) => {
                    error!("Error obteniendo texto OCR focalizado: {}", e);
                    let err_str = e.to_string();
                    if err_str.contains("too small") || err_str.contains("cannot be recognized") {
                        info!("Tile numero {}/{} página {}: Tesseract error (imagen pequeña), saltando", row + 1, tile_rows, i + 1);
                        y += h;
                        continue;
                    }
                    return Err(MediaError::OCRError(err_str));
                }
            };

            let normalized_tile_text = Self::normalize_numero_variants(&tile_text);
            let normalized_numero_text = Self::normalize_numero_variants(&tile_numero_text);

            info!(
                "Tile {}/{} página {}: {:?}",
                row + 1,
                tile_rows,
                i + 1,
                tile_text.trim()
            );

            if let Some(caps) = numero_regex.captures(&normalized_numero_text) {
                info!("Patrón Nº detectado en OCR focalizado: Nº{}", &caps[1]);
            } else if let Some(caps) = numero_regex.captures(&normalized_tile_text) {
                info!("Patrón Nº detectado en OCR general: Nº{}", &caps[1]);
            }

            if !normalized_tile_text.trim().is_empty() {
                full_text.push_str(normalized_tile_text.trim());
                full_text.push('\n');
            }
            y += h;
        }

        Ok(full_text)
    }

    /// Extrae todos los números de factura encontrados: Nº + número
    fn extract_numero_factura(text: &str) -> Vec<String> {
        let re = match Regex::new(r"(?i)\bN[º°o]?\s*([0-9]{1,8})\b") {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        re.captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Extrae todos los RUTs del deudor encontrados (formato chileno: XX.XXX.XXX-K o XX.XXX.XXX-X)
    fn extract_rut_deudor(text: &str) -> Vec<String> {
        let rut_pattern = match Regex::new(r"(\d{1,2}\.\d{3}\.\d{3}-[\dkK])") {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        // Buscar RUTs que estén cerca de palabras como SEÑOR, DEUDOR, etc.
        let context_pattern = match Regex::new(
            r"(?i)(?:DEUDOR|SEÑOR(?:ES)?(?:\s*\(ES\))?|NOMBRE)\s*(?::|\.)*\s*(\d{1,2}\.\d{3}\.\d{3}-[\dkK])",
        ) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        // Primero intentar encontrar RUTs en contexto de SEÑOR/DEUDOR
        let mut ruts: Vec<String> = context_pattern
            .captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect();

        // Si no encuentra en contexto, agregar todos los RUTs encontrados
        if ruts.is_empty() {
            ruts = rut_pattern
                .captures_iter(text)
                .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
                .collect();
        }

        ruts
    }

    /// Extrae todos los nombres del deudor encontrados (aparecen después de SEÑOR(ES): o SEÑORES (ES):)
    fn extract_nombre_deudor(text: &str) -> Vec<String> {
        let patterns = vec![
            r"(?i)SEÑOR(?:ES)?(?:\s*\(ES\))?:\s*([^:\n]+?)(?:\n|$|RUT|rut)",
            r"(?i)SEÑORES\s*\(ES\):\s*([^:\n]+?)(?:\n|$|RUT|rut)",
            r"(?i)NOMBRE\s*(?:DEL)?(?:\s+DEUDOR)?:\s*([^:\n]+?)(?:\n|$|RUT|rut)",
        ];

        let mut nombres = vec![];
        for pattern_str in patterns {
            if let Ok(re) = Regex::new(pattern_str) {
                for caps in re.captures_iter(text) {
                    if let Some(m) = caps.get(1) {
                        let nombre = m.as_str().trim();
                        if !nombre.is_empty() && nombre.len() > 2 {
                            nombres.push(nombre.to_string());
                        }
                    }
                }
            }
        }
        nombres
    }

    /// Extrae todos los montos totales encontrados (aparecen después de "Total $")
    /// Maneja confusiones de OCR: $ puede leerse como 5, S, s, 8
    fn extract_monto_total(text: &str) -> Vec<String> {
        let patterns = vec![
            r"(?i)TOTAL\s+[S$5s8]\s*([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?)",
            r"(?i)TOTAL\s*:\s*[S$5s8]\s*([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?)",
            r"(?i)MONTO\s+TOTAL\s+[S$5s8]\s*([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?)",
        ];

        let mut montos = vec![];
        for pattern_str in patterns {
            if let Ok(re) = Regex::new(pattern_str) {
                for caps in re.captures_iter(text) {
                    if let Some(m) = caps.get(1) {
                        let monto = m.as_str().trim().replace('.', "");
                        montos.push(monto);
                    }
                }
            }
        }
        montos
    }

    fn to_binary(gray: GrayImage, threshold: u8) -> GrayImage {
        let (w, h) = gray.dimensions();
        let mut out = GrayImage::new(w, h);

        for y in 0..h {
            for x in 0..w {
                let p = gray.get_pixel(x, y)[0];
                let v = if p > threshold { 255 } else { 0 };
                out.put_pixel(x, y, Luma([v]));
            }
        }
        out
    }

    fn normalize_numero_variants(input: &str) -> String {
        let re = match Regex::new(r"(?i)\bN\s*(?:[º°oO]|o)?\s*([0-9]{1,8})\b") {
            Ok(v) => v,
            Err(_) => return input.to_string(),
        };
        re.replace_all(input, "Nº$1").to_string()
    }

    /// Normaliza líneas individuales de OCR manteniendo la estructura de array
    /// Cada línea se normaliza por separado, evitando expansión de texto
    fn normalize_ocr_lines(input: &str) -> Vec<String> {
        input
            .lines()
            .map(|line| {
                let normalized = line.trim();

                // Manejar confusiones comunes de OCR
                let normalized = normalized
                    .replace("seÑor", "señor")
                    .replace("seNor", "señor")
                    .replace("SENOR", "SEÑOR");

                // Limpiar espacios alrededor de puntuación
                let normalized = normalized
                    .replace(" :", ":")
                    .replace(": ", ":")
                    .replace(" ,", ",")
                    .replace(" .", ".")
                    .replace(". ", ".")
                    .replace(" - ", "-")
                    .replace(" -", "-")
                    .replace("- ", "-");

                // Limpiar espacios múltiples dentro de la línea
                let re_spaces = match Regex::new(r"\s+") {
                    Ok(re) => re,
                    Err(_) => return normalized,
                };
                re_spaces.replace_all(&normalized, " ").to_string()
            })
            .filter(|line| !line.is_empty()) // Descartar líneas vacías
            .collect()
    }
}
