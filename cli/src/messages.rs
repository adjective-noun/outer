//! WebSocket message types for Outer.sh protocol
//!
//! These types mirror the server's protocol. Some fields may not be used
//! directly by the CLI but are part of the complete protocol.

#![allow(dead_code)]

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Type of block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    User,
    Assistant,
}

/// Status of a block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockStatus {
    Pending,
    Streaming,
    Complete,
    Error,
}

/// Participant information for presence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: Uuid,
    pub name: String,
    pub kind: ParticipantKind,
    pub status: ParticipantStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_block_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantKind {
    User,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    Idle,
    Typing,
    Thinking,
}

/// Messages from client to server
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Submit a prompt
    Submit {
        journal_id: Uuid,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    /// Create a new journal
    CreateJournal {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// Get a journal with its blocks
    GetJournal { journal_id: Uuid },
    /// List all journals
    ListJournals,
    /// Fork a block
    Fork {
        block_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    /// Re-run a block
    Rerun {
        block_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },
    /// Cancel a streaming block
    Cancel { block_id: Uuid },
    /// Subscribe to a journal
    Subscribe {
        journal_id: Uuid,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
    },
    /// Unsubscribe from a journal
    Unsubscribe { journal_id: Uuid },
    /// Update cursor position
    Cursor {
        journal_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        block_id: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        offset: Option<u32>,
    },
    /// Request presence information
    GetPresence { journal_id: Uuid },
}

/// Messages from server to client
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Journal was created
    JournalCreated { journal_id: Uuid, title: String },
    /// Journal with blocks
    Journal { journal: Journal, blocks: Vec<Block> },
    /// List of journals
    Journals { journals: Vec<Journal> },
    /// Block was created
    BlockCreated { block: Block },
    /// Block content delta (streaming)
    BlockContentDelta { block_id: Uuid, delta: String },
    /// Block status changed
    BlockStatusChanged { block_id: Uuid, status: BlockStatus },
    /// Block was forked
    BlockForked {
        original_block_id: Uuid,
        new_block: Block,
    },
    /// Block was cancelled
    BlockCancelled { block_id: Uuid },
    /// Error occurred
    Error { message: String },
    /// Successfully subscribed
    Subscribed {
        journal_id: Uuid,
        participant: Participant,
        participants: Vec<Participant>,
    },
    /// Unsubscribed
    Unsubscribed { journal_id: Uuid },
    /// Participant joined
    ParticipantJoined {
        journal_id: Uuid,
        participant: Participant,
    },
    /// Participant left
    ParticipantLeft {
        journal_id: Uuid,
        participant_id: Uuid,
    },
    /// Cursor moved
    CursorMoved {
        journal_id: Uuid,
        participant_id: Uuid,
        block_id: Option<Uuid>,
        offset: Option<u32>,
    },
    /// Participant status changed
    ParticipantStatusChanged {
        journal_id: Uuid,
        participant_id: Uuid,
        status: ParticipantStatus,
    },
    /// Presence information
    Presence {
        journal_id: Uuid,
        participants: Vec<Participant>,
    },
    /// CRDT update
    CrdtUpdate {
        journal_id: Uuid,
        source: Option<Uuid>,
        update: String,
    },
    /// Sync state
    SyncState { journal_id: Uuid, state: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_submit_serialization() {
        let msg = ClientMessage::Submit {
            journal_id: Uuid::nil(),
            content: "Hello".to_string(),
            session_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("submit"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_server_message_block_created_deserialization() {
        let json = r#"{
            "type": "block_created",
            "block": {
                "id": "00000000-0000-0000-0000-000000000000",
                "journal_id": "00000000-0000-0000-0000-000000000000",
                "block_type": "user",
                "content": "Test",
                "status": "complete",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        }"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::BlockCreated { block } => {
                assert_eq!(block.content, "Test");
                assert_eq!(block.block_type, BlockType::User);
            }
            _ => panic!("Expected BlockCreated"),
        }
    }

    #[test]
    fn test_server_message_error_deserialization() {
        let json = r#"{"type": "error", "message": "Something went wrong"}"#;
        let msg: ServerMessage = serde_json::from_str(json).unwrap();
        match msg {
            ServerMessage::Error { message } => {
                assert_eq!(message, "Something went wrong");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_block_status_values() {
        assert_eq!(
            serde_json::to_string(&BlockStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&BlockStatus::Streaming).unwrap(),
            "\"streaming\""
        );
        assert_eq!(
            serde_json::to_string(&BlockStatus::Complete).unwrap(),
            "\"complete\""
        );
        assert_eq!(
            serde_json::to_string(&BlockStatus::Error).unwrap(),
            "\"error\""
        );
    }
}
