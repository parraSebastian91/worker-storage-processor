use crate::domain::{
    errors::storage_error::RepositoryError,
    ports::outbound::object_storage_repository::IObjectStorageRepository,
};
use crate::infraestructure::adapter::outbound::storage::minio::minio_client::MinioClientAdapter;
use async_trait::async_trait;
use minio::s3::response::GetObjectResponse;
use minio::s3::types::S3Api;

#[async_trait]
impl IObjectStorageRepository for MinioClientAdapter {
    async fn upload_file(
        &self,
        _bucket: &str,
        _key: &str,
        _data: Vec<u8>,
    ) -> Result<(), RepositoryError> {
        // Implementación de subida de archivo a Minio
        Ok(())
    }

    async fn download_file(&self, _bucket: &str, _key: &str) -> Result<Vec<u8>, RepositoryError> {
        let resp: GetObjectResponse = self
            .client()
            .get_object(self.bucket(), _key)
            .send()
            .await
            .map_err(|e| RepositoryError::RetrieveError(e.to_string()))?;
        let content_bytes = resp
            .content
            .to_segmented_bytes()
            .await
            .map_err(|e| RepositoryError::RetrieveError(e.to_string()))?
            .to_bytes();

        Ok(content_bytes.to_vec())
    }

    async fn delete_file(&self, _bucket: &str, _key: &str) -> Result<(), RepositoryError> {
        // Implementación de eliminación de archivo en Minio
        Ok(())
    }

    async fn exists_file(&self, _bucket: &str, _key: &str) -> Result<bool, RepositoryError> {
        // Implementación de verificación de existencia de archivo en Minio
        Ok(false)
    }
}
