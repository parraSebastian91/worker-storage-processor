use serde::Deserialize;

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

#[derive(Debug, Clone, Deserialize)]
pub struct MediaSizeModel {
    // Ejemplo: "sm", "md", "lg"
    pub size: String,
    pub width: i32,
    pub height: i32,
    // Ejemplo: "webp"
    pub format: String,
    pub priority: i32,
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
