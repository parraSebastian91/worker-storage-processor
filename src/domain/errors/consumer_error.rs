#[derive(Debug, thiserror::Error)]
pub enum ConsumerError {
    #[error("Error de conexión: {0}")]
    ConnectionError(String),
    
    #[error("Error al consumir mensaje: {0}")]
    ConsumerError(String),
    
    #[error("Error al hacer ACK: {0}")]
    AckError(String),
    
    #[error("Error al hacer NACK: {0}")]
    NackError(String),
}