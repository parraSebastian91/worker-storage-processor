use serde::{Deserialize, Serialize};

fn default_media_format() -> String {
    "webp".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[serde(default = "default_media_format")]
    pub format: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(skip_deserializing, skip_serializing)]
    pub bytes: Vec<u8>, // Puedes usar un tipo específico si tienes una estructura definida para los metadatos
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageModel {
    pub asset_id: String,
    pub owner_uuid: String,
    pub gestor: String,
    pub media_type: String,
    pub category_process: String,
    pub name_file: String,
    pub format_file: String,
    pub storage_key: String,
}

// PublishPayload — cambiar el tipo de recipe:
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PublishPayload {
    pub event: StorageModel,
    pub recipe: Option<Recipe>, // <-- antes: Option<serde_json::Value>
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
    pub size: String,   // sm, md, lg ...
    pub width: i32,
    pub height: i32,
    pub headers: String, // Cualquier otro metadato relevante
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentRecipeModel {
    pub name: String,
    pub ocr_language: String,
    #[serde(default)]
    pub target_size: Vec<MediaSizeModel>,
    pub category: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Recipe {
    Image(RecipeMediaModel),
    Document(DocumentRecipeModel),
}
