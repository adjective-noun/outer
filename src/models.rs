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
