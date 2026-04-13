use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use tracing::info;

pub struct PostgresClient {
    pool: PgPool,
    database_url: String,
}

impl PostgresClient {
    pub async fn new(database_url: String, max_connections: u32) -> Result<Self, String> {
        info!("Creando conexión a Postgres");

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&database_url)
            .await
            .map_err(|error| format!("Error conectando a Postgres: {}", error))?;

        Ok(Self { pool, database_url })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub async fn ping(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| format!("Error en ping de Postgres: {}", error))
    }
}