use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use lapin::{Connection, ConnectionProperties};
use redis::{cmd, Client as RedisClient};
use serde::Serialize;
use sqlx::Connection as _;
use sqlx::PgConnection;
use tokio::time::{timeout, Duration};
use tracing::{error, info};

use crate::infraestructure::config::app_config::AppConfig;

#[derive(Clone)]
pub struct HealthcheckState {
    pub config: Arc<AppConfig>,
}

#[derive(Serialize)]
struct DependencyStatus {
    status: String,
    message: String,
}

#[derive(Serialize)]
struct HealthcheckResponse {
    status: String,
    redis: DependencyStatus,
    rabbitmq: DependencyStatus,
    postgresql: DependencyStatus,
}

#[derive(Serialize)]
struct LivenessResponse {
    status: String,
}

pub async fn liveness() -> impl IntoResponse {
    info!("Liveness OK ✅ - servicio activo");
    (
        StatusCode::OK,
        Json(LivenessResponse {
            status: "up".to_string(),
        }),
    )
}

pub async fn readiness(State(state): State<HealthcheckState>) -> Response {
    let redis = check_redis(&state.config).await;
    let rabbitmq = check_rabbitmq(&state.config).await;
    let postgresql = check_postgresql(&state.config).await;

    let is_healthy = redis.status == "up" && rabbitmq.status == "up" && postgresql.status == "up";

    if is_healthy {
        info!("Readiness OK ✅ - servicios: redis, rabbitmq, postgresql");
    } else {
        if redis.status != "up" {
            error!(
                "Readiness FAIL ❌ - servicio redis con problemas: {}",
                redis.message
            );
        }
        if rabbitmq.status != "up" {
            error!(
                "Readiness FAIL ❌ - servicio rabbitmq con problemas: {}",
                rabbitmq.message
            );
        }
        if postgresql.status != "up" {
            error!(
                "Readiness FAIL ❌ - servicio postgresql con problemas: {}",
                postgresql.message
            );
        }
    }

    let response = HealthcheckResponse {
        status: if is_healthy {
            "Readiness OK".to_string()
        } else {
            "Readiness error".to_string()
        },
        redis,
        rabbitmq,
        postgresql,
    };

    let status_code = if is_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response)).into_response()
}

async fn check_redis(config: &AppConfig) -> DependencyStatus {
    let cache = &config.cache_config;
    let connection_string = if cache.password.is_empty() {
        format!("redis://{}:{}/{}", cache.host, cache.port, cache.db)
    } else {
        format!(
            "redis://:{}@{}:{}/{}",
            cache.password, cache.host, cache.port, cache.db
        )
    };

    let check = async {
        let client = RedisClient::open(connection_string.as_str()).map_err(|e| e.to_string())?;
        let mut connection = client
            .get_connection_manager()
            .await
            .map_err(|e| e.to_string())?;
        let pong: String = cmd("PING")
            .query_async(&mut connection)
            .await
            .map_err(|e| e.to_string())?;
        Ok::<String, String>(pong)
    };

    match timeout(Duration::from_secs(3), check).await {
        Ok(Ok(pong)) => DependencyStatus {
            status: "up".to_string(),
            message: format!("PONG: {}", pong),
        },
        Ok(Err(error)) => DependencyStatus {
            status: "down".to_string(),
            message: error,
        },
        Err(_) => DependencyStatus {
            status: "down".to_string(),
            message: "timeout al conectar con Redis".to_string(),
        },
    }
}

async fn check_rabbitmq(config: &AppConfig) -> DependencyStatus {
    let url = config.queue_config.url.clone();

    let check = async {
        let connection = Connection::connect(&url, ConnectionProperties::default())
            .await
            .map_err(|e| e.to_string())?;
        connection
            .close(200, "healthcheck")
            .await
            .map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    };

    match timeout(Duration::from_secs(3), check).await {
        Ok(Ok(())) => DependencyStatus {
            status: "up".to_string(),
            message: "Conexión RabbitMQ OK".to_string(),
        },
        Ok(Err(error)) => DependencyStatus {
            status: "down".to_string(),
            message: error,
        },
        Err(_) => DependencyStatus {
            status: "down".to_string(),
            message: "timeout al conectar con RabbitMQ".to_string(),
        },
    }
}

async fn check_postgresql(config: &AppConfig) -> DependencyStatus {
    let postgres_config = &config.db_config;

    let connection_string = format!(
        "postgres://{}:{}@{}:{}/{}",
        postgres_config.user,
        postgres_config.password,
        postgres_config.host,
        postgres_config.port,
        postgres_config.database
    );

    let check = async {
        let mut connection = PgConnection::connect(&connection_string)
            .await
            .map_err(|e: sqlx::Error| e.to_string())?;

        sqlx::query("SELECT 1")
            .execute(&mut connection)
            .await
            .map_err(|e: sqlx::Error| e.to_string())?;

        connection
            .close()
            .await
            .map_err(|e: sqlx::Error| e.to_string())?;
        Ok::<(), String>(())
    };

    match timeout(Duration::from_secs(3), check).await {
        Ok(Ok(())) => DependencyStatus {
            status: "up".to_string(),
            message: "Conexión PostgreSQL OK".to_string(),
        },
        Ok(Err(error)) => DependencyStatus {
            status: "down".to_string(),
            message: error,
        },
        Err(_) => DependencyStatus {
            status: "down".to_string(),
            message: "timeout al conectar con PostgreSQL".to_string(),
        },
    }
}
