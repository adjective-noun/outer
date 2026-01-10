//! WebSocket server handler

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error;
use crate::models::{BlockStatus, BlockType};
use crate::opencode::{OpenCodeClient, SendMessageRequest, StreamEvent};
use crate::AppState;

/// WebSocket handler
pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Get OpenCode URL from environment
    let opencode_url =
        std::env::var("OPENCODE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let opencode = OpenCodeClient::new(opencode_url);

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
        };

        // Parse client message
        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                let error = ServerMessage::Error {
                    message: format!("Invalid message: {}", e),
                };
                if let Err(e) = sender
                    .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                    .await
                {
                    tracing::error!("Failed to send error: {}", e);
                }
                continue;
            }
        };

        // Handle message
        match client_msg {
            ClientMessage::Submit {
                journal_id,
                content,
                session_id,
            } => {
                if let Err(e) = handle_submit(
                    &mut sender,
                    &state,
                    &opencode,
                    journal_id,
                    content,
                    session_id,
                )
                .await
                {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::CreateJournal { title } => {
                match state.store.create_journal(title).await {
                    Ok(journal) => {
                        let msg = ServerMessage::JournalCreated {
                            journal_id: journal.id,
                            title: journal.title,
                        };
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send journal created: {}", e);
                        }
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send error: {}", e);
                        }
                    }
                }
            }
            ClientMessage::GetJournal { journal_id } => {
                match state.store.get_journal(journal_id).await {
                    Ok(journal) => {
                        let blocks = state
                            .store
                            .get_blocks_for_journal(journal_id)
                            .await
                            .unwrap_or_default();
                        let msg = ServerMessage::Journal { journal, blocks };
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send journal: {}", e);
                        }
                    }
                    Err(e) => {
                        let error = ServerMessage::Error {
                            message: e.to_string(),
                        };
                        if let Err(e) = sender
                            .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                            .await
                        {
                            tracing::error!("Failed to send error: {}", e);
                        }
                    }
                }
            }
            ClientMessage::ListJournals => match state.store.list_journals().await {
                Ok(journals) => {
                    let msg = ServerMessage::Journals { journals };
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send journals: {}", e);
                    }
                }
                Err(e) => {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            },
            ClientMessage::Fork {
                block_id,
                session_id,
            } => {
                if let Err(e) =
                    handle_fork(&mut sender, &state, &opencode, block_id, session_id).await
                {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::Rerun {
                block_id,
                session_id,
            } => {
                if let Err(e) =
                    handle_rerun(&mut sender, &state, &opencode, block_id, session_id).await
                {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
            ClientMessage::Cancel { block_id } => {
                if let Err(e) = handle_cancel(&mut sender, &state, block_id).await {
                    let error = ServerMessage::Error {
                        message: e.to_string(),
                    };
                    if let Err(e) = sender
                        .send(Message::Text(serde_json::to_string(&error).unwrap().into()))
                        .await
                    {
                        tracing::error!("Failed to send error: {}", e);
                    }
                }
            }
        }
    }
}

async fn handle_submit(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    journal_id: Uuid,
    content: String,
    session_id: Option<String>,
) -> error::Result<()> {
    // Create user block
    let user_block = state
        .store
        .create_block(journal_id, BlockType::User, &content)
        .await?;

    // Send block created
    let msg = ServerMessage::BlockCreated {
        block: user_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Create assistant block (pending)
    let assistant_block = state
        .store
        .create_block(journal_id, BlockType::Assistant, "")
        .await?;

    let msg = ServerMessage::BlockCreated {
        block: assistant_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Get or create session
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = opencode
                .create_session(crate::opencode::CreateSessionRequest {
                    model: None,
                    system_prompt: None,
                })
                .await?;
            session.id
        }
    };

    // Update block to streaming
    state
        .store
        .update_block_status(assistant_block.id, BlockStatus::Streaming)
        .await?;

    let msg = ServerMessage::BlockStatusChanged {
        block_id: assistant_block.id,
        status: BlockStatus::Streaming,
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Stream response from OpenCode
    let mut stream = opencode
        .send_message(&session_id, SendMessageRequest { content })
        .await?;

    let mut full_content = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::Content(content_event)) => {
                full_content.push_str(&content_event.text);

                // Send streaming update
                let msg = ServerMessage::BlockContentDelta {
                    block_id: assistant_block.id,
                    delta: content_event.text,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Done) => {
                // Update block to complete
                state
                    .store
                    .update_block_content(assistant_block.id, &full_content)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Complete)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Complete,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Error(error_event)) => {
                // Update block to error
                state
                    .store
                    .update_block_content(assistant_block.id, &error_event.message)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Error,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Unknown { .. }) => {
                // Ignore unknown events
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                // Update block to error
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;
            }
        }
    }

    Ok(())
}

async fn handle_fork(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    block_id: Uuid,
    session_id: Option<String>,
) -> error::Result<()> {
    // Fork creates a new user block with the same content, branching from the original
    let forked_block = state.store.fork_block(block_id).await?;

    // Send block forked notification
    let msg = ServerMessage::BlockForked {
        original_block_id: block_id,
        new_block: forked_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Now execute the fork by sending to OpenCode (reusing submit logic)
    // Create assistant block for the response
    let assistant_block = state
        .store
        .create_block_with_lineage(
            forked_block.journal_id,
            BlockType::Assistant,
            "",
            Some(forked_block.id),
            None,
        )
        .await?;

    let msg = ServerMessage::BlockCreated {
        block: assistant_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Get or create session
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = opencode
                .create_session(crate::opencode::CreateSessionRequest {
                    model: None,
                    system_prompt: None,
                })
                .await?;
            session.id
        }
    };

    // Stream response from OpenCode
    stream_response(sender, state, opencode, &session_id, assistant_block, &forked_block.content)
        .await
}

async fn handle_rerun(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    block_id: Uuid,
    session_id: Option<String>,
) -> error::Result<()> {
    // Rerun creates a new execution of the same prompt
    let rerun_block = state.store.rerun_block(block_id).await?;

    // Send block created notification
    let msg = ServerMessage::BlockCreated {
        block: rerun_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Create assistant block for the response
    let assistant_block = state
        .store
        .create_block_with_lineage(
            rerun_block.journal_id,
            BlockType::Assistant,
            "",
            Some(rerun_block.id),
            None,
        )
        .await?;

    let msg = ServerMessage::BlockCreated {
        block: assistant_block.clone(),
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Get or create session
    let session_id = match session_id {
        Some(id) => id,
        None => {
            let session = opencode
                .create_session(crate::opencode::CreateSessionRequest {
                    model: None,
                    system_prompt: None,
                })
                .await?;
            session.id
        }
    };

    // Stream response from OpenCode
    stream_response(sender, state, opencode, &session_id, assistant_block, &rerun_block.content)
        .await
}

async fn handle_cancel(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    block_id: Uuid,
) -> error::Result<()> {
    // Update block status to error (cancelled)
    state
        .store
        .update_block_status(block_id, BlockStatus::Error)
        .await?;

    let msg = ServerMessage::BlockCancelled { block_id };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    Ok(())
}

/// Helper function to stream response from OpenCode
async fn stream_response(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    state: &Arc<AppState>,
    opencode: &OpenCodeClient,
    session_id: &str,
    assistant_block: crate::models::Block,
    content: &str,
) -> error::Result<()> {
    // Update block to streaming
    state
        .store
        .update_block_status(assistant_block.id, BlockStatus::Streaming)
        .await?;

    let msg = ServerMessage::BlockStatusChanged {
        block_id: assistant_block.id,
        status: BlockStatus::Streaming,
    };
    sender
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .map_err(|e| error::AppError::Internal(e.to_string()))?;

    // Stream response from OpenCode
    let mut stream = opencode
        .send_message(session_id, SendMessageRequest {
            content: content.to_string(),
        })
        .await?;

    let mut full_content = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::Content(content_event)) => {
                full_content.push_str(&content_event.text);

                let msg = ServerMessage::BlockContentDelta {
                    block_id: assistant_block.id,
                    delta: content_event.text,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Done) => {
                state
                    .store
                    .update_block_content(assistant_block.id, &full_content)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Complete)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Complete,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Error(error_event)) => {
                state
                    .store
                    .update_block_content(assistant_block.id, &error_event.message)
                    .await?;
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;

                let msg = ServerMessage::BlockStatusChanged {
                    block_id: assistant_block.id,
                    status: BlockStatus::Error,
                };
                sender
                    .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                    .await
                    .map_err(|e| error::AppError::Internal(e.to_string()))?;
            }
            Ok(StreamEvent::Unknown { .. }) => {
                // Ignore unknown events
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                state
                    .store
                    .update_block_status(assistant_block.id, BlockStatus::Error)
                    .await?;
            }
        }
    }

    Ok(())
}

/// Messages from client to server
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Submit a prompt
    Submit {
        journal_id: Uuid,
        content: String,
        session_id: Option<String>,
    },
    /// Create a new journal
    CreateJournal { title: Option<String> },
    /// Get a journal with its blocks
    GetJournal { journal_id: Uuid },
    /// List all journals
    ListJournals,
    /// Fork a block (create new session from a branch point)
    Fork {
        block_id: Uuid,
        session_id: Option<String>,
    },
    /// Re-run a block (same prompt, new execution)
    Rerun {
        block_id: Uuid,
        session_id: Option<String>,
    },
    /// Cancel a streaming block
    Cancel { block_id: Uuid },
}

/// Messages from server to client
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Journal was created
    JournalCreated { journal_id: Uuid, title: String },
    /// Journal with blocks
    Journal {
        journal: crate::models::Journal,
        blocks: Vec<crate::models::Block>,
    },
    /// List of journals
    Journals {
        journals: Vec<crate::models::Journal>,
    },
    /// Block was created
    BlockCreated { block: crate::models::Block },
    /// Block content delta (streaming)
    BlockContentDelta { block_id: Uuid, delta: String },
    /// Block status changed
    BlockStatusChanged {
        block_id: Uuid,
        status: BlockStatus,
    },
    /// Block was forked
    BlockForked {
        original_block_id: Uuid,
        new_block: crate::models::Block,
    },
    /// Block was cancelled
    BlockCancelled { block_id: Uuid },
    /// Error occurred
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_client_message_submit_deserialization() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "submit", "journal_id": "{}", "content": "Hello", "session_id": "sess_123"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Submit {
                journal_id: jid,
                content,
                session_id,
            } => {
                assert_eq!(jid, journal_id);
                assert_eq!(content, "Hello");
                assert_eq!(session_id, Some("sess_123".to_string()));
            }
            _ => panic!("Expected Submit message"),
        }
    }

    #[test]
    fn test_client_message_submit_no_session() {
        let journal_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "submit", "journal_id": "{}", "content": "Test"}}"#,
            journal_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Submit { session_id, .. } => {
                assert_eq!(session_id, None);
            }
            _ => panic!("Expected Submit message"),
        }
    }

    #[test]
    fn test_client_message_create_journal() {
        let json = r#"{"type": "create_journal", "title": "My Journal"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::CreateJournal { title } => {
                assert_eq!(title, Some("My Journal".to_string()));
            }
            _ => panic!("Expected CreateJournal message"),
        }
    }

    #[test]
    fn test_client_message_create_journal_no_title() {
        let json = r#"{"type": "create_journal"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::CreateJournal { title } => {
                assert_eq!(title, None);
            }
            _ => panic!("Expected CreateJournal message"),
        }
    }

    #[test]
    fn test_client_message_get_journal() {
        let journal_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "get_journal", "journal_id": "{}"}}"#, journal_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::GetJournal { journal_id: jid } => {
                assert_eq!(jid, journal_id);
            }
            _ => panic!("Expected GetJournal message"),
        }
    }

    #[test]
    fn test_client_message_list_journals() {
        let json = r#"{"type": "list_journals"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::ListJournals));
    }

    #[test]
    fn test_client_message_invalid_type() {
        let json = r#"{"type": "invalid_type"}"#;
        let result: Result<ClientMessage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_server_message_journal_created() {
        let journal_id = Uuid::new_v4();
        let msg = ServerMessage::JournalCreated {
            journal_id,
            title: "Test".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("journal_created"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_server_message_journal() {
        let journal = crate::models::Journal {
            id: Uuid::nil(),
            title: "Test Journal".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let msg = ServerMessage::Journal {
            journal,
            blocks: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("journal"));
        assert!(json.contains("Test Journal"));
    }

    #[test]
    fn test_server_message_journals() {
        let msg = ServerMessage::Journals { journals: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("journals"));
    }

    #[test]
    fn test_server_message_block_created() {
        let block = crate::models::Block {
            id: Uuid::nil(),
            journal_id: Uuid::nil(),
            block_type: BlockType::User,
            content: "Hello".to_string(),
            status: BlockStatus::Complete,
            parent_id: None,
            forked_from_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let msg = ServerMessage::BlockCreated { block };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_created"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_server_message_block_content_delta() {
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::BlockContentDelta {
            block_id,
            delta: "new content".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_content_delta"));
        assert!(json.contains("new content"));
    }

    #[test]
    fn test_server_message_block_status_changed() {
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::BlockStatusChanged {
            block_id,
            status: BlockStatus::Streaming,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_status_changed"));
        assert!(json.contains("streaming"));
    }

    #[test]
    fn test_server_message_error() {
        let msg = ServerMessage::Error {
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Something went wrong"));
    }

    #[test]
    fn test_server_message_debug() {
        let msg = ServerMessage::Error {
            message: "test".to_string(),
        };
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Error"));
    }

    #[test]
    fn test_client_message_debug() {
        let msg = ClientMessage::ListJournals;
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("ListJournals"));
    }

    #[test]
    fn test_all_block_statuses_in_server_message() {
        let block_id = Uuid::new_v4();

        for status in [
            BlockStatus::Pending,
            BlockStatus::Streaming,
            BlockStatus::Complete,
            BlockStatus::Error,
        ] {
            let msg = ServerMessage::BlockStatusChanged { block_id, status };
            let json = serde_json::to_string(&msg).unwrap();
            assert!(json.contains(status.as_str()));
        }
    }

    #[test]
    fn test_client_message_fork() {
        let block_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "fork", "block_id": "{}", "session_id": "sess_123"}}"#,
            block_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Fork {
                block_id: bid,
                session_id,
            } => {
                assert_eq!(bid, block_id);
                assert_eq!(session_id, Some("sess_123".to_string()));
            }
            _ => panic!("Expected Fork message"),
        }
    }

    #[test]
    fn test_client_message_fork_no_session() {
        let block_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "fork", "block_id": "{}"}}"#, block_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Fork { session_id, .. } => {
                assert_eq!(session_id, None);
            }
            _ => panic!("Expected Fork message"),
        }
    }

    #[test]
    fn test_client_message_rerun() {
        let block_id = Uuid::new_v4();
        let json = format!(
            r#"{{"type": "rerun", "block_id": "{}", "session_id": "sess_456"}}"#,
            block_id
        );
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Rerun {
                block_id: bid,
                session_id,
            } => {
                assert_eq!(bid, block_id);
                assert_eq!(session_id, Some("sess_456".to_string()));
            }
            _ => panic!("Expected Rerun message"),
        }
    }

    #[test]
    fn test_client_message_rerun_no_session() {
        let block_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "rerun", "block_id": "{}"}}"#, block_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Rerun { session_id, .. } => {
                assert_eq!(session_id, None);
            }
            _ => panic!("Expected Rerun message"),
        }
    }

    #[test]
    fn test_client_message_cancel() {
        let block_id = Uuid::new_v4();
        let json = format!(r#"{{"type": "cancel", "block_id": "{}"}}"#, block_id);
        let msg: ClientMessage = serde_json::from_str(&json).unwrap();
        match msg {
            ClientMessage::Cancel { block_id: bid } => {
                assert_eq!(bid, block_id);
            }
            _ => panic!("Expected Cancel message"),
        }
    }

    #[test]
    fn test_server_message_block_forked() {
        let original_block_id = Uuid::new_v4();
        let block = crate::models::Block {
            id: Uuid::nil(),
            journal_id: Uuid::nil(),
            block_type: BlockType::User,
            content: "Forked content".to_string(),
            status: BlockStatus::Pending,
            parent_id: Some(original_block_id),
            forked_from_id: Some(original_block_id),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let msg = ServerMessage::BlockForked {
            original_block_id,
            new_block: block,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_forked"));
        assert!(json.contains("original_block_id"));
        assert!(json.contains("new_block"));
        assert!(json.contains("Forked content"));
    }

    #[test]
    fn test_server_message_block_cancelled() {
        let block_id = Uuid::new_v4();
        let msg = ServerMessage::BlockCancelled { block_id };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("block_cancelled"));
        assert!(json.contains(&block_id.to_string()));
    }
}
