// use aws_sdk_s3::Client;

// pub struct S3StorageImpl {
//     pub client: Client,
//     pub url_base: String,
//     pub bucket: String,
//     pub is_principal: bool,
// }


// impl S3StorageImpl {
//     // Crea una instancia de cliente de storage S3, 
//     // El parámetro is_principal se utiliza para indicar si esta instancia es la principal (true) o una secundaria o de almacenamiento de respaldo (false)
//     pub async fn new(bucket: &str, region: &str, is_principal: bool) -> Result<Self, RepositoryError> {

//         info!(
//             "Inicializando S3Storage - Bucket: {}, Region: {}, IsPrincipal: {}",
//             bucket, region, is_principal
//         );

//         // Configurar el cliente de AWS
//         let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
//             .region(aws_config::Region::new(region.to_string()))
//             .load()
//             .await;

//         let client = Client::new(&config);

//         // Verificar que el bucket existe (opcional)
//         match client.head_bucket().bucket(bucket).send().await {
//             Ok(_) => info!("Bucket {} accesible", bucket),
//             Err(e) => {
//                 error!("Error accediendo al bucket {}: {}", bucket, e);
//                 return Err(RepositoryError::ConnectionError(format!(
//                     "Bucket no accesible: {}",
//                     e
//                 )));
//             }
//         }

//         Ok(Self {
//             client,
//             bucket: bucket.to_string(),
//             url_base: format!("https://{}.s3.amazonaws.com", bucket),
//             is_principal,
//         })
//     }
// }
