use crate::domain::errors::storage_error::RepositoryError;
use async_trait::async_trait;

#[async_trait]
pub trait IObjectDBRepository: Send + Sync {
    async fn save_metadata(&self, _key: &str, _metadata: Vec<u8>) -> Result<(), RepositoryError>;
    async fn get_metadata(&self, _key: &str) -> Result<Option<Vec<u8>>, RepositoryError>;
    async fn delete_metadata(&self, _key: &str) -> Result<(), RepositoryError>;
    async fn exists_metadata(&self, _key: &str) -> Result<bool, RepositoryError>;
}