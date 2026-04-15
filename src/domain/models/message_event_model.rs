use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct RecipeMediaModel {
    pub name: String,
    // Ejemplo: ["sm", "md", "lg"]
    pub target_size: Vec<MediaSizeModel>,
    // Ejemplo: "webp"
    pub format: String,
    pub radio: f64,
    pub priority: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MediaSizeModel {
    // Ejemplo: "sm", "md", "lg"
    pub size: String,
    pub width: i32,
    pub height: i32,
    // Ejemplo: "webp"
    pub format: String,
    pub priority: i32,
    #[serde(skip_deserializing, skip_serializing)]
    pub bytes: Vec<u8>, // Puedes usar un tipo específico si tienes una estructura definida para los metadatos
}
#[derive(Debug, Clone, Deserialize)]
pub struct StorageModel {
    pub asset_id: String,
    pub owner_uuid: String,
    pub media_type: String,
    pub category_process: String,
    pub name_file: String,
    pub format_file: String,
    pub storage_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishPayload {
    pub event: StorageModel,
    pub recipe: RecipeMediaModel,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Representa un mensaje recibido de la cola
#[derive(Debug, Clone)]
pub struct Message {
    /// Tag de entrega para ACK/NACK
    pub delivery_tag: u64,

    /// Contenido del mensaje
    pub body: Vec<u8>,

    /// Headers del mensaje
    pub headers: std::collections::HashMap<String, String>,

    /// Routing key (para RabbitMQ) o partition key
    pub routing_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VariantModel {
    pub asset_id: String,
    pub name: String,
    pub url_path: String,
    pub metadata: VariantMetadataModel, // Puedes usar un tipo específico si tienes una estructura definida para los metadatos
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VariantMetadataModel {
    pub format: String, // webp, jpeg, png, etc.
    pub size: String, // sm, md, lg ...
    pub width: i32,
    pub height: i32,
    pub headers: String, // Cualquier otro metadato relevante
}
