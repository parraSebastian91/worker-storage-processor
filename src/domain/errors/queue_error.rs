#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Error de conexión: {0}")]
    ConnectionError(String),
    
    #[error("Error al consumir mensaje: {0}")]
    ConsumeError(String),
    
    #[error("Error al hacer ACK: {0}")]
    AckError(String),
    
    #[error("Error al hacer NACK: {0}")]
    NackError(String),
}