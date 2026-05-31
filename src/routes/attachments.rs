use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use axum::extract::Multipart;

use crate::db::DbPool;
use crate::error::{AppError, Result};
use crate::models::Agent;
use crate::services::attachment::AttachmentService;

pub async fn upload_attachment(
    State(pool): State<DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Path(action_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>> {
    let svc = AttachmentService::new();
    let mut attachments = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let data = field.bytes().await.map_err(|_| AppError::BadRequest("Failed to read file data".into()))?;

        if data.len() > 10 * 1024 * 1024 {
            return Err(AppError::BadRequest(format!("File too large: {} bytes (max 10MB)", data.len())));
        }

        // Verify the action belongs to this agent
        let owner: Option<(String,)> = sqlx::query_as(
            "SELECT agent_id FROM actions WHERE id = ?"
        )
        .bind(&action_id)
        .fetch_optional(&pool)
        .await
        .map_err(AppError::Database)?;

        match owner {
            Some((aid,)) if aid == agent.id => {}
            _ => return Err(AppError::NotFound("Action not found".into())),
        }

        let attachment = svc.store(&pool, &action_id, &filename, &content_type, &data).await?;

        attachments.push(serde_json::json!({
            "id": attachment.id,
            "filename": attachment.filename,
            "content_type": attachment.content_type,
            "file_size": attachment.file_size,
        }));
    }

    Ok(Json(serde_json::json!({ "attachments": attachments })))
}

pub async fn list_attachments(
    State(pool): State<DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Path(action_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let svc = AttachmentService::new();

    let owner: Option<(String,)> = sqlx::query_as(
        "SELECT agent_id FROM actions WHERE id = ?"
    )
    .bind(&action_id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?;

    match owner {
        Some((aid,)) if aid == agent.id => {}
        _ => return Err(AppError::NotFound("Action not found".into())),
    }

    let attachments = svc.list_for_action(&pool, &action_id).await?;
    let result: Vec<serde_json::Value> = attachments
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "filename": a.filename,
                "content_type": a.content_type,
                "file_size": a.file_size,
                "created_at": a.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

pub async fn download_attachment(
    State(pool): State<DbPool>,
    Path((action_id, attachment_id)): Path<(String, String)>,
) -> Result<axum::response::Response> {
    let svc = AttachmentService::new();
    let attachment = svc.get_by_id(&pool, &attachment_id).await?;

    if attachment.action_id != action_id {
        return Err(AppError::NotFound("Attachment not found".into()));
    }

    let path = svc.file_path(&attachment);
    let data = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::NotFound("File not found on disk".into()))?;

    let content_type: mime::Mime = attachment
        .content_type
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);

    let disposition = format!("attachment; filename=\"{}\"", attachment.filename);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, content_type.to_string())
        .header(axum::http::header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(data))
        .map_err(|_| AppError::Internal)?;

    Ok(response)
}
