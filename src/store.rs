//! Database store for journals and blocks

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{Block, BlockStatus, BlockType, Journal};

/// Database store
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // Journal operations

    pub async fn create_journal(&self, title: Option<String>) -> Result<Journal> {
        let id = Uuid::new_v4();
        let title = title.unwrap_or_else(|| "Untitled".to_string());
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO journals (id, title, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&title)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(Journal {
            id,
            title,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_journal(&self, id: Uuid) -> Result<Journal> {
        let row = sqlx::query_as::<_, JournalRow>(
            r#"
            SELECT id, title, created_at, updated_at
            FROM journals
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Journal {} not found", id)))?;

        row.try_into()
    }

    pub async fn list_journals(&self) -> Result<Vec<Journal>> {
        let rows = sqlx::query_as::<_, JournalRow>(
            r#"
            SELECT id, title, created_at, updated_at
            FROM journals
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    // Block operations

    pub async fn create_block(
        &self,
        journal_id: Uuid,
        block_type: BlockType,
        content: &str,
    ) -> Result<Block> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let status = BlockStatus::Pending;

        sqlx::query(
            r#"
            INSERT INTO blocks (id, journal_id, block_type, content, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(journal_id.to_string())
        .bind(block_type.as_str())
        .bind(content)
        .bind(status.as_str())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Update journal's updated_at
        sqlx::query(
            r#"
            UPDATE journals SET updated_at = ? WHERE id = ?
            "#,
        )
        .bind(now)
        .bind(journal_id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(Block {
            id,
            journal_id,
            block_type,
            content: content.to_string(),
            status,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_block(&self, id: Uuid) -> Result<Block> {
        let row = sqlx::query_as::<_, BlockRow>(
            r#"
            SELECT id, journal_id, block_type, content, status, created_at, updated_at
            FROM blocks
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Block {} not found", id)))?;

        row.try_into()
    }

    pub async fn get_blocks_for_journal(&self, journal_id: Uuid) -> Result<Vec<Block>> {
        let rows = sqlx::query_as::<_, BlockRow>(
            r#"
            SELECT id, journal_id, block_type, content, status, created_at, updated_at
            FROM blocks
            WHERE journal_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(journal_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    pub async fn update_block_content(&self, id: Uuid, content: &str) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE blocks SET content = ?, updated_at = ? WHERE id = ?
            "#,
        )
        .bind(content)
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_block_status(&self, id: Uuid, status: BlockStatus) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE blocks SET status = ?, updated_at = ? WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// Internal row types for sqlx

#[derive(sqlx::FromRow)]
struct JournalRow {
    id: String,
    title: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl TryFrom<JournalRow> for Journal {
    type Error = AppError;

    fn try_from(row: JournalRow) -> Result<Self> {
        Ok(Journal {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| AppError::Internal(format!("Invalid UUID: {}", e)))?,
            title: row.title,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct BlockRow {
    id: String,
    journal_id: String,
    block_type: String,
    content: String,
    status: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl TryFrom<BlockRow> for Block {
    type Error = AppError;

    fn try_from(row: BlockRow) -> Result<Self> {
        Ok(Block {
            id: Uuid::parse_str(&row.id)
                .map_err(|e| AppError::Internal(format!("Invalid UUID: {}", e)))?,
            journal_id: Uuid::parse_str(&row.journal_id)
                .map_err(|e| AppError::Internal(format!("Invalid UUID: {}", e)))?,
            block_type: row
                .block_type
                .parse()
                .map_err(|e| AppError::Internal(format!("Invalid block type: {}", e)))?,
            content: row.content,
            status: row
                .status
                .parse()
                .map_err(|e| AppError::Internal(format!("Invalid status: {}", e)))?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}
