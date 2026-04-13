#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Error de conexión: {0}")]
    ConnectionError(String),

    #[error("Error guardando archivo: {0}")]
    SaveError(String),

    #[error("Error recuperando archivo: {0}")]
    RetrieveError(String),

    #[error("Archivo no encontrado: {0}")]
    NotFound(String),

    #[error("Error eliminando archivo: {0}")]
    DeleteError(String),
}