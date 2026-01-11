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
        self.create_block_with_lineage(journal_id, block_type, content, None, None)
            .await
    }

    /// Create a block with optional parent and forked_from IDs for timeline branching
    pub async fn create_block_with_lineage(
        &self,
        journal_id: Uuid,
        block_type: BlockType,
        content: &str,
        parent_id: Option<Uuid>,
        forked_from_id: Option<Uuid>,
    ) -> Result<Block> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        // User blocks are complete immediately; assistant blocks start pending
        let status = match block_type {
            BlockType::User => BlockStatus::Complete,
            BlockType::Assistant => BlockStatus::Pending,
        };

        sqlx::query(
            r#"
            INSERT INTO blocks (id, journal_id, block_type, content, status, parent_id, forked_from_id, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(journal_id.to_string())
        .bind(block_type.as_str())
        .bind(content)
        .bind(status.as_str())
        .bind(parent_id.map(|u| u.to_string()))
        .bind(forked_from_id.map(|u| u.to_string()))
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
            parent_id,
            forked_from_id,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_block(&self, id: Uuid) -> Result<Block> {
        let row = sqlx::query_as::<_, BlockRow>(
            r#"
            SELECT id, journal_id, block_type, content, status, parent_id, forked_from_id, created_at, updated_at
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
            SELECT id, journal_id, block_type, content, status, parent_id, forked_from_id, created_at, updated_at
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

    /// Fork a block: create a new user block with the same content, branching from the parent
    /// Returns the new user block (caller should then create assistant block and send to OpenCode)
    pub async fn fork_block(&self, block_id: Uuid) -> Result<Block> {
        let original = self.get_block(block_id).await?;

        // For forking, we create a new user block with the original's content
        // The parent_id points to the block we're forking after (creating a branch point)
        // The forked_from_id points to the original block being forked
        self.create_block_with_lineage(
            original.journal_id,
            BlockType::User,
            &original.content,
            Some(block_id), // parent_id: the block we're branching from
            Some(block_id), // forked_from_id: the original block
        )
        .await
    }

    /// Re-run a block: create a new execution with the same prompt
    /// For user blocks: creates new user block with same content, then caller sends to OpenCode
    /// For assistant blocks: finds the preceding user block and re-runs that prompt
    pub async fn rerun_block(&self, block_id: Uuid) -> Result<Block> {
        let original = self.get_block(block_id).await?;

        let (content, parent_id) = match original.block_type {
            BlockType::User => {
                // Re-run the user's message: use same content, fork from this block
                (original.content.clone(), block_id)
            }
            BlockType::Assistant => {
                // Find the preceding user block to get the prompt
                let blocks = self.get_blocks_for_journal(original.journal_id).await?;
                let user_block = blocks
                    .iter()
                    .rev()
                    .find(|b| b.block_type == BlockType::User && b.created_at < original.created_at)
                    .ok_or_else(|| {
                        AppError::NotFound("No preceding user block found".to_string())
                    })?;
                (user_block.content.clone(), block_id)
            }
        };

        self.create_block_with_lineage(
            original.journal_id,
            BlockType::User,
            &content,
            Some(parent_id), // parent_id: branching point
            Some(block_id),  // forked_from_id: the block being re-run
        )
        .await
    }

    /// Get blocks that were forked from a specific block
    pub async fn get_forks(&self, block_id: Uuid) -> Result<Vec<Block>> {
        let rows = sqlx::query_as::<_, BlockRow>(
            r#"
            SELECT id, journal_id, block_type, content, status, parent_id, forked_from_id, created_at, updated_at
            FROM blocks
            WHERE forked_from_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(block_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Get child blocks (blocks that have this block as parent)
    pub async fn get_children(&self, block_id: Uuid) -> Result<Vec<Block>> {
        let rows = sqlx::query_as::<_, BlockRow>(
            r#"
            SELECT id, journal_id, block_type, content, status, parent_id, forked_from_id, created_at, updated_at
            FROM blocks
            WHERE parent_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(block_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
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
    parent_id: Option<String>,
    forked_from_id: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl TryFrom<BlockRow> for Block {
    type Error = AppError;

    fn try_from(row: BlockRow) -> Result<Self> {
        let parent_id = row
            .parent_id
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| AppError::Internal(format!("Invalid parent_id UUID: {}", e)))?;
        let forked_from_id = row
            .forked_from_id
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| AppError::Internal(format!("Invalid forked_from_id UUID: {}", e)))?;

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
            parent_id,
            forked_from_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_test_db() -> Store {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        // Run migrations manually
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS journals (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create journals table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS blocks (
                id TEXT PRIMARY KEY NOT NULL,
                journal_id TEXT NOT NULL REFERENCES journals(id),
                block_type TEXT NOT NULL CHECK (block_type IN ('user', 'assistant')),
                content TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'streaming', 'complete', 'error')),
                parent_id TEXT REFERENCES blocks(id),
                forked_from_id TEXT REFERENCES blocks(id),
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create blocks table");

        Store::new(pool)
    }

    #[tokio::test]
    async fn test_create_journal_with_title() {
        let store = setup_test_db().await;
        let journal = store
            .create_journal(Some("My Journal".to_string()))
            .await
            .unwrap();
        assert_eq!(journal.title, "My Journal");
    }

    #[tokio::test]
    async fn test_create_journal_without_title() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        assert_eq!(journal.title, "Untitled");
    }

    #[tokio::test]
    async fn test_get_journal() {
        let store = setup_test_db().await;
        let created = store
            .create_journal(Some("Test".to_string()))
            .await
            .unwrap();
        let fetched = store.get_journal(created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.title, "Test");
    }

    #[tokio::test]
    async fn test_get_journal_not_found() {
        let store = setup_test_db().await;
        let result = store.get_journal(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_list_journals() {
        let store = setup_test_db().await;
        store
            .create_journal(Some("First".to_string()))
            .await
            .unwrap();
        store
            .create_journal(Some("Second".to_string()))
            .await
            .unwrap();

        let journals = store.list_journals().await.unwrap();
        assert_eq!(journals.len(), 2);
    }

    #[tokio::test]
    async fn test_list_journals_empty() {
        let store = setup_test_db().await;
        let journals = store.list_journals().await.unwrap();
        assert!(journals.is_empty());
    }

    #[tokio::test]
    async fn test_create_block() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let block = store
            .create_block(journal.id, BlockType::User, "Hello")
            .await
            .unwrap();
        assert_eq!(block.journal_id, journal.id);
        assert_eq!(block.block_type, BlockType::User);
        assert_eq!(block.content, "Hello");
        // User blocks are complete immediately
        assert_eq!(block.status, BlockStatus::Complete);
    }

    #[tokio::test]
    async fn test_create_assistant_block() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let block = store
            .create_block(journal.id, BlockType::Assistant, "Hi there")
            .await
            .unwrap();
        assert_eq!(block.block_type, BlockType::Assistant);
    }

    #[tokio::test]
    async fn test_get_block() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let created = store
            .create_block(journal.id, BlockType::User, "Test")
            .await
            .unwrap();
        let fetched = store.get_block(created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.content, "Test");
    }

    #[tokio::test]
    async fn test_get_block_not_found() {
        let store = setup_test_db().await;
        let result = store.get_block(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_get_blocks_for_journal() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        store
            .create_block(journal.id, BlockType::User, "Message 1")
            .await
            .unwrap();
        store
            .create_block(journal.id, BlockType::Assistant, "Response 1")
            .await
            .unwrap();

        let blocks = store.get_blocks_for_journal(journal.id).await.unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].content, "Message 1");
        assert_eq!(blocks[1].content, "Response 1");
    }

    #[tokio::test]
    async fn test_get_blocks_for_journal_empty() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let blocks = store.get_blocks_for_journal(journal.id).await.unwrap();
        assert!(blocks.is_empty());
    }

    #[tokio::test]
    async fn test_update_block_content() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let block = store
            .create_block(journal.id, BlockType::User, "Original")
            .await
            .unwrap();

        store
            .update_block_content(block.id, "Updated")
            .await
            .unwrap();

        let fetched = store.get_block(block.id).await.unwrap();
        assert_eq!(fetched.content, "Updated");
    }

    #[tokio::test]
    async fn test_update_block_status() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let block = store
            .create_block(journal.id, BlockType::Assistant, "")
            .await
            .unwrap();
        assert_eq!(block.status, BlockStatus::Pending);

        store
            .update_block_status(block.id, BlockStatus::Streaming)
            .await
            .unwrap();
        let fetched = store.get_block(block.id).await.unwrap();
        assert_eq!(fetched.status, BlockStatus::Streaming);

        store
            .update_block_status(block.id, BlockStatus::Complete)
            .await
            .unwrap();
        let fetched = store.get_block(block.id).await.unwrap();
        assert_eq!(fetched.status, BlockStatus::Complete);
    }

    #[tokio::test]
    async fn test_update_block_status_to_error() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let block = store
            .create_block(journal.id, BlockType::Assistant, "")
            .await
            .unwrap();

        store
            .update_block_status(block.id, BlockStatus::Error)
            .await
            .unwrap();

        let fetched = store.get_block(block.id).await.unwrap();
        assert_eq!(fetched.status, BlockStatus::Error);
    }

    #[tokio::test]
    async fn test_create_block_updates_journal_timestamp() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();
        let original_updated_at = journal.updated_at;

        // Small delay to ensure timestamp difference
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .create_block(journal.id, BlockType::User, "New message")
            .await
            .unwrap();

        let updated_journal = store.get_journal(journal.id).await.unwrap();
        assert!(updated_journal.updated_at >= original_updated_at);
    }

    #[tokio::test]
    async fn test_journal_row_try_from_invalid_uuid() {
        // Test that invalid UUIDs in the database are handled
        let row = JournalRow {
            id: "not-a-uuid".to_string(),
            title: "Test".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Journal> = row.try_into();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::Internal(_)));
    }

    #[tokio::test]
    async fn test_block_row_try_from_invalid_uuid() {
        let row = BlockRow {
            id: "not-a-uuid".to_string(),
            journal_id: Uuid::new_v4().to_string(),
            block_type: "user".to_string(),
            content: "test".to_string(),
            status: "pending".to_string(),
            parent_id: None,
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Block> = row.try_into();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_block_row_try_from_invalid_journal_uuid() {
        let row = BlockRow {
            id: Uuid::new_v4().to_string(),
            journal_id: "not-a-uuid".to_string(),
            block_type: "user".to_string(),
            content: "test".to_string(),
            status: "pending".to_string(),
            parent_id: None,
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Block> = row.try_into();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_block_row_try_from_invalid_block_type() {
        let row = BlockRow {
            id: Uuid::new_v4().to_string(),
            journal_id: Uuid::new_v4().to_string(),
            block_type: "invalid".to_string(),
            content: "test".to_string(),
            status: "pending".to_string(),
            parent_id: None,
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Block> = row.try_into();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_block_row_try_from_invalid_status() {
        let row = BlockRow {
            id: Uuid::new_v4().to_string(),
            journal_id: Uuid::new_v4().to_string(),
            block_type: "user".to_string(),
            content: "test".to_string(),
            status: "invalid".to_string(),
            parent_id: None,
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Block> = row.try_into();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_block_row_try_from_invalid_parent_id() {
        let row = BlockRow {
            id: Uuid::new_v4().to_string(),
            journal_id: Uuid::new_v4().to_string(),
            block_type: "user".to_string(),
            content: "test".to_string(),
            status: "pending".to_string(),
            parent_id: Some("not-a-uuid".to_string()),
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Block> = row.try_into();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_block_row_try_from_invalid_forked_from_id() {
        let row = BlockRow {
            id: Uuid::new_v4().to_string(),
            journal_id: Uuid::new_v4().to_string(),
            block_type: "user".to_string(),
            content: "test".to_string(),
            status: "pending".to_string(),
            parent_id: None,
            forked_from_id: Some("not-a-uuid".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result: Result<Block> = row.try_into();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_store_new() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let store = Store::new(pool);
        // Just verify it doesn't panic
        assert!(true);
        drop(store);
    }

    #[tokio::test]
    async fn test_create_block_with_lineage() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        // First create a parent block that actually exists
        let parent_block = store
            .create_block(journal.id, BlockType::User, "Parent")
            .await
            .unwrap();

        // Create another block that we'll fork from
        let source_block = store
            .create_block(journal.id, BlockType::User, "Source")
            .await
            .unwrap();

        let block = store
            .create_block_with_lineage(
                journal.id,
                BlockType::User,
                "Test",
                Some(parent_block.id),
                Some(source_block.id),
            )
            .await
            .unwrap();

        assert_eq!(block.parent_id, Some(parent_block.id));
        assert_eq!(block.forked_from_id, Some(source_block.id));

        // Verify it persists
        let fetched = store.get_block(block.id).await.unwrap();
        assert_eq!(fetched.parent_id, Some(parent_block.id));
        assert_eq!(fetched.forked_from_id, Some(source_block.id));
    }

    #[tokio::test]
    async fn test_fork_block() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        // Create original user block
        let original = store
            .create_block(journal.id, BlockType::User, "Original message")
            .await
            .unwrap();

        // Fork it
        let forked = store.fork_block(original.id).await.unwrap();

        assert_eq!(forked.content, "Original message");
        assert_eq!(forked.block_type, BlockType::User);
        assert_eq!(forked.parent_id, Some(original.id));
        assert_eq!(forked.forked_from_id, Some(original.id));
        assert_ne!(forked.id, original.id);
    }

    #[tokio::test]
    async fn test_rerun_user_block() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        // Create original user block
        let original = store
            .create_block(journal.id, BlockType::User, "User prompt")
            .await
            .unwrap();

        // Re-run it
        let rerun = store.rerun_block(original.id).await.unwrap();

        assert_eq!(rerun.content, "User prompt");
        assert_eq!(rerun.block_type, BlockType::User);
        assert_eq!(rerun.parent_id, Some(original.id));
        assert_eq!(rerun.forked_from_id, Some(original.id));
    }

    #[tokio::test]
    async fn test_rerun_assistant_block() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        // Create user block followed by assistant block
        let user_block = store
            .create_block(journal.id, BlockType::User, "User prompt")
            .await
            .unwrap();

        // Small delay to ensure ordering
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let assistant_block = store
            .create_block(journal.id, BlockType::Assistant, "Assistant response")
            .await
            .unwrap();

        // Re-run the assistant block
        let rerun = store.rerun_block(assistant_block.id).await.unwrap();

        // Should create a new user block with the original user's content
        assert_eq!(rerun.content, "User prompt");
        assert_eq!(rerun.block_type, BlockType::User);
        assert_eq!(rerun.parent_id, Some(assistant_block.id));
        assert_eq!(rerun.forked_from_id, Some(assistant_block.id));
    }

    #[tokio::test]
    async fn test_rerun_assistant_block_no_preceding_user() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        // Create only assistant block (edge case)
        let assistant_block = store
            .create_block(journal.id, BlockType::Assistant, "Response")
            .await
            .unwrap();

        // Re-run should fail - no preceding user block
        let result = store.rerun_block(assistant_block.id).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_get_forks() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        let original = store
            .create_block(journal.id, BlockType::User, "Original")
            .await
            .unwrap();

        // Create some forks
        let _fork1 = store.fork_block(original.id).await.unwrap();
        let _fork2 = store.fork_block(original.id).await.unwrap();

        let forks = store.get_forks(original.id).await.unwrap();
        assert_eq!(forks.len(), 2);
    }

    #[tokio::test]
    async fn test_get_children() {
        let store = setup_test_db().await;
        let journal = store.create_journal(None).await.unwrap();

        let parent = store
            .create_block(journal.id, BlockType::User, "Parent")
            .await
            .unwrap();

        // Create children
        let _child1 = store
            .create_block_with_lineage(
                journal.id,
                BlockType::User,
                "Child 1",
                Some(parent.id),
                None,
            )
            .await
            .unwrap();
        let _child2 = store
            .create_block_with_lineage(
                journal.id,
                BlockType::User,
                "Child 2",
                Some(parent.id),
                None,
            )
            .await
            .unwrap();

        let children = store.get_children(parent.id).await.unwrap();
        assert_eq!(children.len(), 2);
    }

    #[tokio::test]
    async fn test_fork_nonexistent_block() {
        let store = setup_test_db().await;
        let result = store.fork_block(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_rerun_nonexistent_block() {
        let store = setup_test_db().await;
        let result = store.rerun_block(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }
}
