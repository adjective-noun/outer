//! OpenCode Bridge - HTTP client for OpenCode server

use futures::stream::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::{AppError, Result};

/// OpenCode client for interacting with the OpenCode server
pub struct OpenCodeClient {
    client: Client,
    base_url: String,
}

impl OpenCodeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Create a new session
    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        let response = self
            .client
            .post(format!("{}/sessions", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::OpenCode(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to create session: {} - {}",
                status, text
            )));
        }

        response
            .json()
            .await
            .map_err(|e| AppError::OpenCode(e.to_string()))
    }

    /// Send a message and stream the response
    pub async fn send_message(
        &self,
        session_id: &str,
        request: SendMessageRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let response = self
            .client
            .post(format!("{}/sessions/{}/messages", self.base_url, session_id))
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::OpenCode(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to send message: {} - {}",
                status, text
            )));
        }

        // Parse SSE stream
        let stream = parse_sse_stream(response);
        Ok(Box::pin(stream))
    }

    /// Subscribe to events for a session
    pub async fn subscribe_events(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let response = self
            .client
            .get(format!("{}/sessions/{}/events", self.base_url, session_id))
            .send()
            .await
            .map_err(|e| AppError::OpenCode(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to subscribe to events: {} - {}",
                status, text
            )));
        }

        let stream = parse_sse_stream(response);
        Ok(Box::pin(stream))
    }
}

/// Parse SSE stream from response
fn parse_sse_stream(
    response: reqwest::Response,
) -> impl Stream<Item = Result<StreamEvent>> + Send {
    use futures::StreamExt;
    use tokio_stream::wrappers::LinesStream;
    use tokio::io::AsyncBufReadExt;

    async_stream::stream! {
        let mut bytes_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut event_type = String::new();
        let mut data = String::new();

        while let Some(chunk) = bytes_stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    yield Err(AppError::OpenCode(e.to_string()));
                    break;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    // Empty line signals end of event
                    if !data.is_empty() {
                        match parse_event(&event_type, &data) {
                            Ok(event) => yield Ok(event),
                            Err(e) => yield Err(e),
                        }
                        event_type.clear();
                        data.clear();
                    }
                } else if let Some(value) = line.strip_prefix("event:") {
                    event_type = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(value.trim());
                }
            }
        }
    }
}

fn parse_event(event_type: &str, data: &str) -> Result<StreamEvent> {
    match event_type {
        "content" | "" => {
            let content: ContentEvent = serde_json::from_str(data)
                .map_err(|e| AppError::OpenCode(format!("Failed to parse content event: {}", e)))?;
            Ok(StreamEvent::Content(content))
        }
        "done" => Ok(StreamEvent::Done),
        "error" => {
            let error: ErrorEvent = serde_json::from_str(data)
                .map_err(|e| AppError::OpenCode(format!("Failed to parse error event: {}", e)))?;
            Ok(StreamEvent::Error(error))
        }
        _ => {
            // Unknown event type, return raw
            Ok(StreamEvent::Unknown {
                event_type: event_type.to_string(),
                data: data.to_string(),
            })
        }
    }
}

// Request/Response types

#[derive(Debug, Serialize)]
pub struct CreateSessionRequest {
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Session {
    pub id: String,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct SendMessageRequest {
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Content(ContentEvent),
    Done,
    Error(ErrorEvent),
    Unknown { event_type: String, data: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentEvent {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErrorEvent {
    pub message: String,
    pub code: Option<String>,
}
