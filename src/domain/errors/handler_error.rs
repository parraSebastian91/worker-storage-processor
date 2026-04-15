#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("Error de deserialización: {0}")]
    DeserializationError(String),

    #[error("Error de procesamiento: {0}")]
    ProcessingError(String),

    #[error("Error de almacenamiento: {0}")]
    RepositoryError(String),

    #[error("Error de validación: {0}")]
    ValidationError(String),

    #[error("Documento no encontrado: {0}")]
    DocumentNotFound(String),

    #[error("Tipo de medio no soportado: {0}")]
    UnsupportedMediaType(String),
}
