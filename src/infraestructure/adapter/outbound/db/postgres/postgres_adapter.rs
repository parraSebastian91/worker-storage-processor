use crate::{
    domain::{
        errors::storage_error::RepositoryError,
        models::{
            media_status_enum::MediaStatus,
            media_status_enum::MediaStatus::{Deprecated,Ready}, message_event_model::VariantModel,
        },
        ports::outbound::object_db_repository::IObjectDBRepository,

    },
    infraestructure::adapter::outbound::db::postgres::postgres_client::PostgresClient,
};
use async_trait::async_trait;
use sqlx::types::Json;
use uuid::Uuid;
use tracing::info;

#[async_trait]
impl IObjectDBRepository for PostgresClient {
    async fn save_metadata(&self, _key: &str, _metadata: Vec<u8>) -> Result<(), RepositoryError> {
        // Aquí iría la lógica para guardar los metadatos en Postgres, por ejemplo:
        // 1. Convertir el ObjectMetadata al formato adecuado para la base de datos
        // 2. Ejecutar una consulta SQL para insertar o actualizar los metadatos
        // 3. Manejar errores específicos y retornar un RepositoryError si algo falla

        // let sql = "SELECT id, asset_id, variant_name, url_path, metadata, created_at FROM media.media_variants";

        Ok(())
    }

    async fn get_metadata(&self, _key: &str) -> Result<Option<Vec<u8>>, RepositoryError> {
        // Aquí iría la lógica para obtener los metadatos desde Postgres, por ejemplo:
        // 1. Ejecutar una consulta SQL para recuperar los metadatos por ID
        // 2. Convertir el resultado de la consulta al formato ObjectMetadata
        // 3. Manejar errores específicos y retornar un RepositoryError si algo falla

        Ok(None)
    }

    async fn delete_metadata(&self, _key: &str) -> Result<(), RepositoryError> {
        // Aquí iría la lógica para eliminar los metadatos desde Postgres, por ejemplo:
        // 1. Ejecutar una consulta SQL para eliminar los metadatos por ID
        // 2. Manejar errores específicos y retornar un RepositoryError si algo falla

        Ok(())
    }

    async fn exists_metadata(&self, _key: &str) -> Result<bool, RepositoryError> {
        // Aquí iría la lógica para verificar la existencia de los metadatos en Postgres, por ejemplo:
        // 1. Ejecutar una consulta SQL para contar los registros con el ID dado
        // 2. Retornar true si el conteo es mayor que 0, o false si es 0
        // 3. Manejar errores específicos y retornar un RepositoryError si algo falla

        Ok(false)
    }

    async fn update_state(&self, _key: &str, _state: MediaStatus) -> Result<(), RepositoryError> {
        let id = Uuid::parse_str(_key)
            .map_err(|e| RepositoryError::NotFound(format!("UUID inválido '{}': {}", _key, e)))?;

        let sql = "UPDATE media.media_assets SET status = $1 WHERE id = $2";

        let result = sqlx::query(sql)
            .bind(_state)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                RepositoryError::ConnectionError(format!("Error ejecutando UPDATE: {}", e))
            })?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "Activo no encontrado: {}",
                _key
            )));
        }
        Ok(())
    }

    async fn create_variant(&self, media: VariantModel) -> Result<(), RepositoryError> {
        let id = Uuid::parse_str(&media.asset_id).map_err(|e| {
            RepositoryError::NotFound(format!("UUID inválido '{}': {}", &media.asset_id, e))
        })?;
        let metadata_json = Json(media.metadata.clone());

        let sql = "INSERT INTO media.media_variants (asset_id, variant_name, url_path, metadata) VALUES ($1, $2, $3, $4)";

        let result = sqlx::query(sql)
            .bind(id)
            .bind(&media.name)
            .bind(&media.url_path)
            .bind(metadata_json)
            .execute(self.pool())
            .await
            .map_err(|e| {
                RepositoryError::ConnectionError(format!("Error ejecutando INSERT: {}", e))
            })?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::SaveError(format!(
                "Error al insertar variante: {}",
                media.asset_id
            )));
        }
        Ok(())
    }

    async fn update_state_and_key_storage(
        &self,
        _key: &str,
        _new_key: &str,
        _state: MediaStatus,
    ) -> Result<(), RepositoryError> {
        let id = Uuid::parse_str(_key)
            .map_err(|e| RepositoryError::NotFound(format!("UUID inválido '{}': {}", _key, e)))?;

        let sql = "UPDATE media.media_assets SET status = $1 WHERE id = $2";

        let result = sqlx::query(sql)
            .bind(_state)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                RepositoryError::ConnectionError(format!("Error ejecutando UPDATE: {}", e))
            })?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "Activo no encontrado: {}",
                _key
            )));
        }
        Ok(())
    }

    async fn deprecate_old_assets(&self, _key: &str, _category: &str) -> Result<(), RepositoryError> {
        let id = Uuid::parse_str(_key)
            .map_err(|e| RepositoryError::NotFound(format!("UUID inválido '{}': {}", _key, e)))?;

        let sql = "update media.media_assets set status = $1 WHERE category = $2 and owner_id = $3 and status = $4";

        let result = sqlx::query(sql)
            .bind(Deprecated)
            .bind(_category)
            .bind(id)
            .bind(Ready)
            .execute(self.pool())   
            .await
            .map_err(|e| {
                RepositoryError::ConnectionError(format!("Error ejecutando UPDATE: {}", e))
            })?;

        if result.rows_affected() == 0 {
            info!(
                "No existian assets antiguos para la categoria: {} usuario: {}",
                _category, _key
            );
        }
        Ok(())
    }
}
