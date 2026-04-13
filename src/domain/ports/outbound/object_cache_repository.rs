use async_trait::async_trait;

use crate::domain::errors::storage_error::RepositoryError;

#[async_trait]
pub trait IObjectCacheRepository {
    async fn set(&self, key: &str, value: Vec<u8>, ttl_seconds: usize) ->Result<(), RepositoryError>;
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, RepositoryError>;
    async fn delete(&self, key: &str) -> Result<(), RepositoryError>;
    async fn exists(&self, key: &str) -> Result<bool, RepositoryError>;
}