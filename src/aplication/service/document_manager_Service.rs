#[cfg(feature = "ocr")]
use std::io::Cursor;

#[cfg(feature = "ocr")]
use image::ImageFormat;
#[cfg(feature = "ocr")]
use pdfium_render::prelude::{PdfRenderConfig, Pdfium};
#[cfg(feature = "ocr")]
use tesseract::Tesseract;

use crate::domain::errors::media_error::MediaError;

pub struct DocumentManagerService {}

impl DocumentManagerService {
    pub fn new() -> Self {
        Self {}
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
        let bindings = Pdfium::bind_to_system_library()
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;
        let pdfium = Pdfium::new(bindings);

        let document = pdfium
            .load_pdf_from_byte_vec(pdf_bytes.to_vec(), None)
            .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

        let mut full_text = String::new();

        for page in document.pages().iter() {
            let page_image = page
                .render_with_config(
                    &PdfRenderConfig::new()
                        .set_target_width(2000)
                        .render_form_data(true),
                )
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?
                .as_image();

            let mut cursor = Cursor::new(Vec::new());
            page_image
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(|e| MediaError::PdfRenderError(e.to_string()))?;

            let page_ocr_text = Tesseract::new(None, Some(language))
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .set_image_from_mem(cursor.get_ref())
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .recognize()
                .map_err(|e| MediaError::OCRError(e.to_string()))?
                .get_text()
                .map_err(|e| MediaError::OCRError(e.to_string()))?;

            if !page_ocr_text.trim().is_empty() {
                full_text.push_str(page_ocr_text.trim());
                full_text.push('\n');
            }
        }

        Ok(full_text)
        }
    }
}
