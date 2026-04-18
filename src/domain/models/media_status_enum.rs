#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "media.media_status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MediaStatus {
    Pending,
    Uploaded,
    Processing,
    Ready,
    Error,
    Deprecated,
}