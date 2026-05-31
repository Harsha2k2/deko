use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::attachment::Attachment;

pub struct AttachmentService {
    pub storage_dir: PathBuf,
}

impl AttachmentService {
    pub fn new() -> Self {
        let dir = PathBuf::from("static/uploads");
        std::fs::create_dir_all(&dir).ok();
        Self { storage_dir: dir }
    }

    pub async fn store(
        &self,
        pool: &DbPool,
        action_id: &str,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<Attachment> {
        let id = Uuid::new_v4().to_string();
        let safe_name = sanitize_filename(filename);
        let storage_name = format!("{}_{}", &id[..8], safe_name);
        let storage_path = self.storage_dir.join(&storage_name);

        fs::write(&storage_path, data)
            .await
            .map_err(|_| AppError::Internal)?;

        let file_size = data.len() as i64;
        let clean_filename = sanitize_filename(filename);
        let storage_name_clone = storage_name.clone();

        sqlx::query(
            "INSERT INTO attachments (id, action_id, filename, content_type, file_size, storage_path) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(action_id)
        .bind(&clean_filename)
        .bind(content_type)
        .bind(file_size)
        .bind(&storage_name)
        .execute(pool)
        .await
        .map_err(AppError::Database)?;

        Ok(Attachment {
            id,
            action_id: action_id.to_string(),
            filename: clean_filename,
            content_type: content_type.to_string(),
            file_size,
            storage_path: storage_name_clone,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn list_for_action(
        &self,
        pool: &DbPool,
        action_id: &str,
    ) -> Result<Vec<Attachment>> {
        let attachments = sqlx::query_as::<_, Attachment>(
            "SELECT id, action_id, filename, content_type, file_size, storage_path, created_at FROM attachments WHERE action_id = ? ORDER BY created_at ASC",
        )
        .bind(action_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::Database)?;

        Ok(attachments)
    }

    pub async fn get_by_id(
        &self,
        pool: &DbPool,
        id: &str,
    ) -> Result<Attachment> {
        sqlx::query_as::<_, Attachment>(
            "SELECT id, action_id, filename, content_type, file_size, storage_path, created_at FROM attachments WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Attachment not found".into()))
    }

    pub fn file_path(&self, attachment: &Attachment) -> PathBuf {
        self.storage_dir.join(&attachment.storage_path)
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
