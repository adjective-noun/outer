//! Data models for journals and blocks

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A journal represents a conversation/session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A block represents a single message/turn in a journal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: Uuid,
    pub journal_id: Uuid,
    pub block_type: BlockType,
    pub content: String,
    pub status: BlockStatus,
    /// Parent block ID for timeline branching (the block this was forked after)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    /// Original block ID that was forked/re-run to create this block
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Type of block (user message or assistant response)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    User,
    Assistant,
}

impl BlockType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockType::User => "user",
            BlockType::Assistant => "assistant",
        }
    }
}

impl std::str::FromStr for BlockType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(BlockType::User),
            "assistant" => Ok(BlockType::Assistant),
            _ => Err(format!("Invalid block type: {}", s)),
        }
    }
}

/// Status of a block (for streaming responses)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockStatus {
    Pending,
    Streaming,
    Complete,
    Error,
}

impl BlockStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlockStatus::Pending => "pending",
            BlockStatus::Streaming => "streaming",
            BlockStatus::Complete => "complete",
            BlockStatus::Error => "error",
        }
    }
}

impl std::str::FromStr for BlockStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(BlockStatus::Pending),
            "streaming" => Ok(BlockStatus::Streaming),
            "complete" => Ok(BlockStatus::Complete),
            "error" => Ok(BlockStatus::Error),
            _ => Err(format!("Invalid block status: {}", s)),
        }
    }
}

/// Request to create a new journal
#[derive(Debug, Deserialize)]
pub struct CreateJournalRequest {
    pub title: Option<String>,
}

/// Request to create a new block
#[derive(Debug, Deserialize)]
pub struct CreateBlockRequest {
    pub journal_id: Uuid,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_type_as_str() {
        assert_eq!(BlockType::User.as_str(), "user");
        assert_eq!(BlockType::Assistant.as_str(), "assistant");
    }

    #[test]
    fn test_block_type_from_str() {
        assert_eq!("user".parse::<BlockType>().unwrap(), BlockType::User);
        assert_eq!(
            "assistant".parse::<BlockType>().unwrap(),
            BlockType::Assistant
        );
    }

    #[test]
    fn test_block_type_from_str_invalid() {
        let result = "invalid".parse::<BlockType>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid block type: invalid");
    }

    #[test]
    fn test_block_status_as_str() {
        assert_eq!(BlockStatus::Pending.as_str(), "pending");
        assert_eq!(BlockStatus::Streaming.as_str(), "streaming");
        assert_eq!(BlockStatus::Complete.as_str(), "complete");
        assert_eq!(BlockStatus::Error.as_str(), "error");
    }

    #[test]
    fn test_block_status_from_str() {
        assert_eq!(
            "pending".parse::<BlockStatus>().unwrap(),
            BlockStatus::Pending
        );
        assert_eq!(
            "streaming".parse::<BlockStatus>().unwrap(),
            BlockStatus::Streaming
        );
        assert_eq!(
            "complete".parse::<BlockStatus>().unwrap(),
            BlockStatus::Complete
        );
        assert_eq!("error".parse::<BlockStatus>().unwrap(), BlockStatus::Error);
    }

    #[test]
    fn test_block_status_from_str_invalid() {
        let result = "invalid".parse::<BlockStatus>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid block status: invalid");
    }

    #[test]
    fn test_journal_serialization() {
        let journal = Journal {
            id: Uuid::nil(),
            title: "Test".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&journal).unwrap();
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_block_serialization() {
        let block = Block {
            id: Uuid::nil(),
            journal_id: Uuid::nil(),
            block_type: BlockType::User,
            content: "Hello".to_string(),
            status: BlockStatus::Complete,
            parent_id: None,
            forked_from_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("user"));
        assert!(json.contains("complete"));
        // Optional fields should be skipped when None
        assert!(!json.contains("parent_id"));
        assert!(!json.contains("forked_from_id"));
    }

    #[test]
    fn test_block_serialization_with_fork_fields() {
        let parent_id = Uuid::new_v4();
        let forked_from_id = Uuid::new_v4();
        let block = Block {
            id: Uuid::nil(),
            journal_id: Uuid::nil(),
            block_type: BlockType::User,
            content: "Forked".to_string(),
            status: BlockStatus::Complete,
            parent_id: Some(parent_id),
            forked_from_id: Some(forked_from_id),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("parent_id"));
        assert!(json.contains("forked_from_id"));
        assert!(json.contains(&parent_id.to_string()));
        assert!(json.contains(&forked_from_id.to_string()));
    }

    #[test]
    fn test_block_type_serde_rename() {
        let json = r#""user""#;
        let block_type: BlockType = serde_json::from_str(json).unwrap();
        assert_eq!(block_type, BlockType::User);

        let json = r#""assistant""#;
        let block_type: BlockType = serde_json::from_str(json).unwrap();
        assert_eq!(block_type, BlockType::Assistant);
    }

    #[test]
    fn test_block_status_serde_rename() {
        let json = r#""pending""#;
        let status: BlockStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, BlockStatus::Pending);

        let json = r#""streaming""#;
        let status: BlockStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status, BlockStatus::Streaming);
    }

    #[test]
    fn test_create_journal_request_deserialization() {
        let json = r#"{"title": "My Journal"}"#;
        let req: CreateJournalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, Some("My Journal".to_string()));

        let json = r#"{}"#;
        let req: CreateJournalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, None);
    }

    #[test]
    fn test_create_block_request_deserialization() {
        let id = Uuid::new_v4();
        let json = format!(r#"{{"journal_id": "{}", "content": "test"}}"#, id);
        let req: CreateBlockRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.journal_id, id);
        assert_eq!(req.content, "test");
    }
}
