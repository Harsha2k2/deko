use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Attachment {
    pub id: String,
    pub action_id: String,
    pub filename: String,
    pub content_type: String,
    pub file_size: i64,
    pub storage_path: String,
    pub created_at: String,
}
