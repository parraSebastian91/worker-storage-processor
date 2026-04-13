
use dotenvy::dotenv;
use serde::Deserialize;
use std::{env, sync::Arc};

use crate::domain::errors::config_error::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub configuracion_gral: ConfiguracionGral,
    pub queue_config: QueueConfig,
    pub db_config: DbConfig,
    pub cache_config: CacheConfig,
    pub minio_config: MinioConfig,
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfiguracionGral {
    pub port: String,
    pub app_name: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    pub url: String,
    pub queue: String,
    pub prefetch_count: u16,
    pub exchange: String,
    pub routing_key: String,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub host: String,
    pub password: String,
    pub port: u16,
    pub db: u8,
    pub ttl: usize, // TTL en segundos
}

#[derive(Debug, Clone, Deserialize)]
pub struct MinioConfig {
    pub url_base: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub is_principal: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    /// Nivel de log (trace, debug, info, warn, error)
    pub log_level: String,

    /// Formato de log (json, pretty, compact)
    pub log_format: String,

    /// Habilitar métricas
    pub enable_metrics: bool,

    /// Puerto para métricas Prometheus
    pub metrics_port: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Arc<Self>, ConfigError> {
        let _ = dotenv();

        let configuracion_gral = ConfiguracionGral {
            port: env::var("PORT").unwrap_or_else(|_| "8080".to_string()),
            app_name: env::var("APP_NAME")
                .unwrap_or_else(|_| "worker-storage-processor".to_string()),
        };

        let queue_config = QueueConfig {
            url: env::var("QUEUE_URL").map_err(|_| ConfigError::MissingEnvVar("QUEUE_URL is not set".to_string()))?,
            queue: env::var("QUEUE_NAME").map_err(|_| ConfigError::MissingEnvVar("QUEUE_NAME is not set".to_string()))?,
            prefetch_count: env::var("QUEUE_PREFETCH_COUNT")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid QUEUE_PREFETCH_COUNT".to_string()))?,
            exchange: env::var("QUEUE_EXCHANGE")
                .or_else(|_| env::var("QUEUE_EXCHANGE_NAME"))
                .unwrap_or_else(|_| "".to_string()),
            routing_key: env::var("QUEUE_ROUTING_KEY").unwrap_or_else(|_| "".to_string()),
            max_retries: env::var("QUEUE_MAX_RETRIES")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid QUEUE_MAX_RETRIES".to_string()))?,
        };

        let db_config = DbConfig {
            host: env::var("DB_HOST").map_err(|_| ConfigError::MissingEnvVar("DB_HOST is not set".to_string()))?,
            port: env::var("DB_PORT")
                .unwrap_or_else(|_| "5432".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid DB_PORT".to_string()))?,
            user: env::var("DB_USER").map_err(|_| ConfigError::MissingEnvVar("DB_USER is not set".to_string()))?,
            password: env::var("DB_PASSWORD").map_err(|_| ConfigError::MissingEnvVar("DB_PASSWORD is not set".to_string()))?,
            database: env::var("DB_NAME").map_err(|_| ConfigError::MissingEnvVar("DB_NAME is not set".to_string()))?,
            max_connections: env::var("DB_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid DB_MAX_CONNECTIONS".to_string()))?,
        };

        let minio_config = MinioConfig {
            url_base: env::var("MINIO_URL_BASE")
                .map_err(|_| ConfigError::MissingEnvVar("MINIO_URL_BASE is not set".to_string()))?,
            bucket: env::var("MINIO_BUCKET").map_err(|_| ConfigError::MissingEnvVar("MINIO_BUCKET is not set".to_string()))?,
            access_key: env::var("MINIO_ACCESS_KEY")
                .map_err(|_| ConfigError::MissingEnvVar("MINIO_ACCESS_KEY is not set".to_string()))?,
            secret_key: env::var("MINIO_SECRET_KEY")
                .map_err(|_| ConfigError::MissingEnvVar("MINIO_SECRET_KEY is not set".to_string()))?,
            is_principal: env::var("MINIO_IS_PRINCIPAL")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid MINIO_IS_PRINCIPAL".to_string()))?,
        };

        let cache_config = CacheConfig {
            host: env::var("CACHE_HOST").map_err(|_| ConfigError::MissingEnvVar("CACHE_HOST is not set".to_string()))?,
            password: env::var("CACHE_PASSWORD")
                .map_err(|_| ConfigError::MissingEnvVar("CACHE_PASSWORD is not set".to_string()))?,
            port: env::var("CACHE_PORT")
                .unwrap_or_else(|_| "6379".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid CACHE_PORT".to_string()))?,
            db: env::var("CACHE_DB")
                .unwrap_or_else(|_| "0".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid CACHE_DB".to_string()))?,
            ttl: env::var("CACHE_TTL")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .map_err(|_| ConfigError::MissingEnvVar("Invalid CACHE_TTL".to_string()))?,
        };

        let observability = ObservabilityConfig {
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            log_format: env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string()),
            enable_metrics: env::var("ENABLE_METRICS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            metrics_port: env::var("METRICS_PORT")
                .unwrap_or_else(|_| "9090".to_string())
                .parse()
                .unwrap_or(9090),
        };

        Ok(Arc::new(Self {
            configuracion_gral,
            queue_config,
            db_config,
            cache_config,
            minio_config,
            observability,
        }))
    }
}
