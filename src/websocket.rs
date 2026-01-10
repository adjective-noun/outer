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
) -> crate::error::Result<()> {
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
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

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
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

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
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

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
                    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
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
                    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
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
                    .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
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
    /// Error occurred
    Error { message: String },
}
