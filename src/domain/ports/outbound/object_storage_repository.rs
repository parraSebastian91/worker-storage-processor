use async_trait::async_trait;

use crate::domain::errors::storage_error::RepositoryError;   

#[async_trait]
pub trait IObjectStorageRepository: Send + Sync {
    async fn upload_file(&self, bucket: &str, key: &str, data: Vec<u8>) -> Result<(), RepositoryError>;
    async fn download_file(&self, bucket: &str, key: &str) -> Result<Vec<u8>, RepositoryError>;
    async fn delete_file(&self, bucket: &str, key: &str) -> Result<(), RepositoryError>;
    async fn exists_file(&self, bucket: &str, key: &str) -> Result<bool, RepositoryError>;
}