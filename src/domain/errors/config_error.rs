/// Errores de configuración
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Variable de entorno faltante: {0}")]
    MissingEnvVar(String),

    #[error("Tipo de mensajería inválido: {0}")]
    InvalidMessagingType(String),

    #[error("Tipo de almacenamiento inválido: {0}")]
    InvalidStorageType(String),

    #[error("Error de parsing: {0}")]
    ParseError(String),
}
