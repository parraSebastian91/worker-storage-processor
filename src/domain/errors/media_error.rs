#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Error de procesamiento de imagen: {0}")]
    ImageProcessingError(String),

    #[error("Error de procesamiento de video: {0}")]
    VideoProcessingError(String),

    #[error("Error de procesamiento de otro tipo de medio: {0}")]
    OtherMediaProcessingError(String),

    #[error("Error de OCR: {0}")]
    OCRError(String),

    #[error("Error de renderizado PDF: {0}")]
    PdfRenderError(String),

    #[error("Dimensiones inválidas: {0}")]
    InvalidDimensions(String),
}
