use crate::{domain::{
    errors::storage_error::RepositoryError,
    ports::outbound::object_db_repository::IObjectDBRepository,
}, infraestructure::adapter::outbound::db::postgres::postgres_client::PostgresClient};
use async_trait::async_trait;

#[async_trait]
impl IObjectDBRepository for PostgresClient {
    async fn save_metadata(&self, _key: &str, _metadata: Vec<u8>) -> Result<(), RepositoryError> {
        // Aquí iría la lógica para guardar los metadatos en Postgres, por ejemplo:
        // 1. Convertir el ObjectMetadata al formato adecuado para la base de datos
        // 2. Ejecutar una consulta SQL para insertar o actualizar los metadatos
        // 3. Manejar errores específicos y retornar un RepositoryError si algo falla

        let sql = "SELECT id, asset_id, variant_name, url_path, metadata, created_at FROM media.media_variants";

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
}
