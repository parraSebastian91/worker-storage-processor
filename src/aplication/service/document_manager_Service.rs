#[cfg(feature = "ocr")]
use std::io::Cursor;

#[cfg(feature = "ocr")]
use image::ImageFormat;
#[cfg(feature = "ocr")]
use image::DynamicImage;
use image::{GrayImage, Luma};
#[cfg(feature = "ocr")]
use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
use regex::Regex;
#[cfg(feature = "ocr")]
use tesseract::Tesseract;

use crate::domain::errors::media_error::MediaError;
#[cfg(feature = "ocr")]
use tracing::{error, info};
use crate::domain::models::factura_data_model::InvoiceData;

pub struct DocumentManagerService {}

impl DocumentManagerService {
    pub fn new() -> Self {
        Self {}
    }

    pub fn render_first_page_png_from_pdf(&self, pdf_bytes: &[u8]) -> Result<Vec<u8>, MediaError> {
        #[cfg(not(feature = "ocr"))]
        {
            let _ = pdf_bytes;
            return Err(MediaError::OCRError(
                "OCR no habilitado. Compila con --features ocr y asegura Tesseract/Leptonica instalados"
                    .to_string(),
            ));
        }

        #[cfg(feature = "ocr")]
        {
            let page_image = self.render_first_page_image(pdf_bytes)?;

            let mut png_cursor = Cursor::new(Vec::new());
            page_image
                .write_to(&mut png_cursor, ImageFormat::Png)
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

            Ok(png_cursor.into_inner())
        }
    }

    #[cfg(feature = "ocr")]
    fn render_first_page_image(&self, pdf_bytes: &[u8]) -> Result<DynamicImage, MediaError> {
        let bindings = Pdfium::bind_to_system_library()
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
        let pdfium = Pdfium::new(bindings);

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
