//! WebSocket client for Outer.sh server

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::messages::{BlockStatus, ClientMessage, Journal, ServerMessage};

/// WebSocket client for Outer.sh
pub struct OuterClient {
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<ServerMessage>,
    #[allow(dead_code)]
    handle: tokio::task::JoinHandle<()>,
}

impl OuterClient {
    /// Connect to an Outer.sh server
    pub async fn connect(url: &str) -> Result<Self> {
        tracing::info!("Connecting to {}", url);

        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Channel for outgoing messages
        let (out_tx, mut out_rx) = mpsc::channel::<Message>(32);

        // Channel for incoming parsed messages
        let (in_tx, in_rx) = mpsc::channel::<ServerMessage>(32);

        // Spawn task to handle WebSocket communication
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Handle outgoing messages
                    Some(msg) = out_rx.recv() => {
                        if write.send(msg).await.is_err() {
                            break;
                        }
                    }
                    // Handle incoming messages
                    Some(result) = read.next() => {
                        match result {
                            Ok(Message::Text(text)) => {
                                match serde_json::from_str::<ServerMessage>(&text) {
                                    Ok(msg) => {
                                        if in_tx.send(msg).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to parse message: {} - {}", e, text);
                                    }
                                }
                            }
                            Ok(Message::Close(_)) => break,
                            Err(e) => {
                                tracing::error!("WebSocket error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                    else => break,
                }
            }
        });

        tracing::info!("Connected successfully");

        Ok(Self {
            tx: out_tx,
            rx: in_rx,
            handle,
        })
    }

    /// Send a message to the server
    async fn send(&self, msg: ClientMessage) -> Result<()> {
        let json = serde_json::to_string(&msg)?;
        self.tx
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| anyhow!("Failed to send message: {}", e))
    }

    /// Receive a message from the server
    pub async fn recv(&mut self) -> Option<ServerMessage> {
        self.rx.recv().await
    }

    /// Try to receive a message without blocking
    pub fn try_recv(&mut self) -> Option<ServerMessage> {
        self.rx.try_recv().ok()
    }

    /// Create a new journal
    pub async fn create_journal(&mut self, title: Option<String>) -> Result<Journal> {
        self.send(ClientMessage::CreateJournal { title }).await?;

        while let Some(msg) = self.recv().await {
            match msg {
                ServerMessage::JournalCreated { journal_id, title } => {
                    return Ok(Journal {
                        id: journal_id,
                        title,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    });
                }
                ServerMessage::Error { message } => {
                    return Err(anyhow!("Server error: {}", message));
                }
                _ => continue,
            }
        }

        Err(anyhow!("Connection closed"))
    }

    /// List all journals
    pub async fn list_journals(&mut self) -> Result<Vec<Journal>> {
        self.send(ClientMessage::ListJournals).await?;

        while let Some(msg) = self.recv().await {
            match msg {
                ServerMessage::Journals { journals } => {
                    return Ok(journals);
                }
                ServerMessage::Error { message } => {
                    return Err(anyhow!("Server error: {}", message));
                }
                _ => continue,
            }
        }

        Err(anyhow!("Connection closed"))
    }

    /// Get a journal with its blocks
    pub async fn get_journal(
        &mut self,
        journal_id: Uuid,
    ) -> Result<(Journal, Vec<crate::messages::Block>)> {
        self.send(ClientMessage::GetJournal { journal_id }).await?;

        while let Some(msg) = self.recv().await {
            match msg {
                ServerMessage::Journal { journal, blocks } => {
                    return Ok((journal, blocks));
                }
                ServerMessage::Error { message } => {
                    return Err(anyhow!("Server error: {}", message));
                }
                _ => continue,
            }
        }

        Err(anyhow!("Connection closed"))
    }

    /// Subscribe to a journal for real-time updates
    pub async fn subscribe(
        &mut self,
        journal_id: Uuid,
        name: String,
        kind: Option<String>,
    ) -> Result<(
        crate::messages::Participant,
        Vec<crate::messages::Participant>,
    )> {
        self.send(ClientMessage::Subscribe {
            journal_id,
            name,
            kind,
        })
        .await?;

        while let Some(msg) = self.recv().await {
            match msg {
                ServerMessage::Subscribed {
                    participant,
                    participants,
                    ..
                } => {
                    return Ok((participant, participants));
                }
                ServerMessage::Error { message } => {
                    return Err(anyhow!("Server error: {}", message));
                }
                _ => continue,
            }
        }

        Err(anyhow!("Connection closed"))
    }

    /// Submit a message and stream the response
    pub async fn submit_and_stream<F>(
        &mut self,
        journal_id: Uuid,
        content: String,
        mut callback: F,
    ) -> Result<()>
    where
        F: FnMut(ServerMessage),
    {
        self.send(ClientMessage::Submit {
            journal_id,
            content,
            session_id: None,
        })
        .await?;

        // Wait for the assistant block to be created and completed
        let mut assistant_block_id: Option<Uuid> = None;

        while let Some(msg) = self.recv().await {
            match &msg {
                ServerMessage::BlockCreated { block } => {
                    if block.block_type == crate::messages::BlockType::Assistant {
                        assistant_block_id = Some(block.id);
                    }
                    callback(msg);
                }
                ServerMessage::BlockStatusChanged { block_id, status } => {
                    callback(msg.clone());
                    if Some(*block_id) == assistant_block_id
                        && (*status == BlockStatus::Complete || *status == BlockStatus::Error)
                    {
                        return Ok(());
                    }
                }
                ServerMessage::Error { message } => {
                    return Err(anyhow!("Server error: {}", message));
                }
                _ => {
                    callback(msg);
                }
            }
        }

        Err(anyhow!("Connection closed"))
    }

    /// Fork a block and stream the response
    pub async fn fork_and_stream<F>(&mut self, block_id: Uuid, mut callback: F) -> Result<()>
    where
        F: FnMut(ServerMessage),
    {
        self.send(ClientMessage::Fork {
            block_id,
            session_id: None,
        })
        .await?;

        // Wait for the assistant block to be created and completed
        let mut assistant_block_id: Option<Uuid> = None;

        while let Some(msg) = self.recv().await {
            match &msg {
                ServerMessage::BlockForked { .. } => {
                    callback(msg);
                }
                ServerMessage::BlockCreated { block } => {
                    if block.block_type == crate::messages::BlockType::Assistant {
                        assistant_block_id = Some(block.id);
                    }
                    callback(msg);
                }
                ServerMessage::BlockStatusChanged { block_id, status } => {
                    callback(msg.clone());
                    if Some(*block_id) == assistant_block_id
                        && (*status == BlockStatus::Complete || *status == BlockStatus::Error)
                    {
                        return Ok(());
                    }
                }
                ServerMessage::Error { message } => {
                    return Err(anyhow!("Server error: {}", message));
                }
                _ => {
                    callback(msg);
                }
            }
        }

        Err(anyhow!("Connection closed"))
    }

    /// Submit a message (non-blocking, returns immediately)
    pub async fn submit(&mut self, journal_id: Uuid, content: String) -> Result<()> {
        self.send(ClientMessage::Submit {
            journal_id,
            content,
            session_id: None,
        })
        .await
    }

    /// Cancel a streaming block
    pub async fn cancel(&mut self, block_id: Uuid) -> Result<()> {
        self.send(ClientMessage::Cancel { block_id }).await
    }

    /// Listen for events until callback returns false
    pub async fn listen<F>(&mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(ServerMessage) -> bool,
    {
        while let Some(msg) = self.recv().await {
            if !callback(msg) {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::ListJournals;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("list_journals"));
    }

    #[test]
    fn test_submit_message_serialization() {
        let msg = ClientMessage::Submit {
            journal_id: Uuid::nil(),
            content: "Hello".to_string(),
            session_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("submit"));
        assert!(json.contains("Hello"));
        assert!(!json.contains("session_id")); // Should be skipped when None
    }
}
