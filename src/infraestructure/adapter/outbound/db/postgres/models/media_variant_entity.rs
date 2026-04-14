use uuid::Uuid;

pub struct MediaVariantEntity {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub variant_name: String,
    pub url_path: String,
    pub metadata: serde_json::Value, // Puedes usar un tipo específico si tienes una estructura definida para los metadatos
    pub created_at: chrono::NaiveDateTime,
}
