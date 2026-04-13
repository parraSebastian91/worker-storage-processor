//! Redis Client Implementation
use redis::{aio::ConnectionManager, Client};
use tracing::{error, info};

use crate::domain::errors::storage_error::RepositoryError;

pub struct RedisCacheImpl {
    pub connection: ConnectionManager,
    pub ttl: usize, // TTL en segundos
}

impl RedisCacheImpl {
    /// Crea una nueva instancia del cache desde parámetros individuales
    pub async fn new(
        host: &str,
        port: u16,
        password: &str,
        db: u8,
        ttl: usize,
    ) -> Result<Self, RepositoryError> {
        info!("Conectando a Redis en {}:{}/{}", host, port, db);
        // Construir connection string
        // let connection_string = if password.is_empty() {
        //     format!("redis://{}:{}/{}", host, port, db)
        // } else {
        //     format!("redis://:{}@{}:{}/{}", password, host, port, db)
        // };
       let connection_string =  format!("redis://{}:{}/{}", host, port, db);

        let client = Client::open(connection_string).map_err(|e| {
            error!("Error creando cliente Redis: {}", e);
            RepositoryError::ConnectionError(e.to_string())
        })?;

        let connection = client.get_connection_manager().await.map_err(|e| {
            error!("Error conectando a Redis: {}", e);
            RepositoryError::ConnectionError(e.to_string())
        })?;

        info!("Conectado a Redis exitosamente");

        Ok(Self { connection, ttl })
    }

    /// Genera la clave de cache para un documento
    pub fn get_cache_key(code_document: &str) -> String {
        format!("document:{}", code_document)
    }
}
