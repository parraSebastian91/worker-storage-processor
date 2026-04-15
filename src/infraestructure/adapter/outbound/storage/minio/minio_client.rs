use minio::s3::client::Client;
use minio::s3::creds::StaticProvider;
use minio::s3::http::BaseUrl;

use tracing::info;

use crate::domain::errors::storage_error::RepositoryError;

pub struct MinioClientAdapter {
    client: Client,
    url_base: String,
    bucket: String,
    is_principal: bool,
}

impl MinioClientAdapter{
    pub async fn new(
        url_base: String,
        bucket: String,
        access_key: String,
        secret_key: String,
        is_principal: bool,
    ) -> Result<Self, RepositoryError> {
        info!(
            "Crando conexión al servicio de almacenamiento Minio en {}",
            url_base
        );

        let endpoint = if url_base.starts_with("http://") || url_base.starts_with("https://") {
            url_base.clone()
        } else {
            format!("http://{}", url_base)
        };

        let base_url: BaseUrl = match endpoint.parse() {
            Ok(base_url) => base_url,
            Err(error) => {
                return Err(RepositoryError::ConnectionError(error.to_string()));
            }
        };

        let static_provider = StaticProvider::new(&access_key, &secret_key, None);

        let client = match Client::new(base_url, Some(Box::new(static_provider)), None, None) {
            Ok(client) => client,
            Err(error) => {
                return Err(RepositoryError::ConnectionError(error.to_string()));
            }
        };

        Ok(Self {
            client,
            url_base,
            bucket,
            is_principal,
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn url_base(&self) -> &str {
        &self.url_base
    }

    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub fn is_principal(&self) -> bool {
        self.is_principal
    }
}
