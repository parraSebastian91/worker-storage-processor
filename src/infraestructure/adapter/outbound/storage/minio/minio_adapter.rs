use crate::domain::{
    errors::storage_error::RepositoryError,
    ports::outbound::object_storage_repository::IObjectStorageRepository,
};
use crate::infraestructure::adapter::outbound::storage::minio::minio_client::MinioClientAdapter;
use async_trait::async_trait;
use bytes::Bytes;
use minio::s3::response::GetObjectResponse;
use minio::s3::response::PutObjectResponse;
use minio::s3::segmented_bytes::SegmentedBytes;
use minio::s3::types::S3Api;
use tracing::{debug};
#[async_trait]
impl IObjectStorageRepository for MinioClientAdapter {
    async fn upload_file(
        &self,
        _bucket: &str,
        _key: &str,
        _data: Vec<u8>,
        private: bool,
    ) -> Result<(), RepositoryError> {
        let data = SegmentedBytes::from(Bytes::from(_data));
        let bucket = if private { &self.bucket().private_processed } else { &self.bucket().public_processed };
        debug!("Subiendo archivo a Minio: bucket={}, key={}", bucket, _key);
        let _resp: PutObjectResponse = self
            .client()
            .put_object(bucket, _key, data)
            .send()
            .await
            .map_err(|e| RepositoryError::SaveError(e.to_string()))?;
        Ok(())
    }

    async fn download_file(
        &self,
        _bucket: &str,
        _key: &str,
        private: bool,
    ) -> Result<Vec<u8>, RepositoryError> {
        let bucket = if private { &self.bucket().private_original } else { &self.bucket().public_original };
        debug!("Descargando archivo de Minio: bucket={}, key={}", bucket, _key);
        let resp: GetObjectResponse = self
            .client()
            .get_object(bucket, _key)
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

    async fn delete_file(&self, _bucket: &str, _key: &str, private: bool) -> Result<(), RepositoryError> {
        // Implementación de eliminación de archivo en Minio
        let bucket = if private { &self.bucket().private_original } else { &self.bucket().public_original };
        self.client()
            .delete_object(bucket, _key)
            .send()
            .await
            .map_err(|e| RepositoryError::DeleteError(e.to_string()))?;
        Ok(())
    }

    async fn exists_file(&self, _bucket: &str, _key: &str, private: bool) -> Result<bool, RepositoryError> {
        // Implementación de verificación de existencia de archivo en Minio
        let bucket = if private { &self.bucket().private_original } else { &self.bucket().public_original };
        match self.client().stat_object(bucket, _key).send().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
