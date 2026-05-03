use sqlx::SqlitePool;

use crate::services::openai::OpenAIClient;

pub struct VerdictService {
    pub pool: SqlitePool,
    pub openai: OpenAIClient,
}

impl VerdictService {
    pub fn new(pool: SqlitePool, openai: OpenAIClient) -> Self {
        Self { pool, openai }
    }
}
