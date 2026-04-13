use crate::{
    domain::{
        errors::storage_error::RepositoryError,
        ports::outbound::object_cache_repository::IObjectCacheRepository,
    },
    infraestructure::adapter::outbound::db::cache::cache_client::RedisCacheImpl,
};
use async_trait::async_trait;
use redis::AsyncCommands;
use tracing::{error, info};

#[async_trait]
impl IObjectCacheRepository for RedisCacheImpl {
    async fn set(&self, key: &str, value: Vec<u8>, ttl_seconds: usize) -> Result<(), RepositoryError> {
        let cache_key = RedisCacheImpl::get_cache_key(key);
        let mut conn = self.connection.clone();
        match conn
            .set_ex::<_, _, ()>(&cache_key, value, ttl_seconds as u64)
            .await
        {
            Ok(_) => {
                info!("Documento guardado en cache: {}", cache_key);
                Ok(())
            }
            Err(e) => {
                error!("Error guardando en Redis: {}", e);
                Err(RepositoryError::SaveError(e.to_string()))
            }
        }
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, RepositoryError> {
        let cache_key = RedisCacheImpl::get_cache_key(key);
        let mut conn = self.connection.clone();
        let value: Option<Vec<u8>> = conn
            .get(&cache_key)
            .await
            .map_err(|e| RepositoryError::RetrieveError(e.to_string()))?;
        Ok(value)
    }

    async fn delete(&self, key: &str) -> Result<(), RepositoryError> {
        let cache_key = RedisCacheImpl::get_cache_key(key);
        let mut conn = self.connection.clone();
        conn.del::<_, usize>(&cache_key)
            .await
            .map_err(|e| RepositoryError::DeleteError(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, RepositoryError> {
        let cache_key = RedisCacheImpl::get_cache_key(key);
        let mut conn = self.connection.clone();
        let exists: bool = conn.exists(&cache_key).await.map_err(|e| {
            error!("Error verificando existencia en Redis: {}", e);
            RepositoryError::RetrieveError(e.to_string())
        })?;
        Ok(exists)
    }
}
