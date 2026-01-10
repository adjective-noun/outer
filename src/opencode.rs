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

pub(crate) fn parse_event(event_type: &str, data: &str) -> Result<StreamEvent> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_client_new() {
        let client = OpenCodeClient::new("http://localhost:8080");
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn test_opencode_client_new_with_string() {
        let url = String::from("http://example.com:3000");
        let client = OpenCodeClient::new(url);
        assert_eq!(client.base_url, "http://example.com:3000");
    }

    #[test]
    fn test_parse_event_content() {
        let data = r#"{"text": "Hello, world!"}"#;
        let event = parse_event("content", data).unwrap();
        match event {
            StreamEvent::Content(content) => {
                assert_eq!(content.text, "Hello, world!");
            }
            _ => panic!("Expected Content event"),
        }
    }

    #[test]
    fn test_parse_event_content_empty_type() {
        let data = r#"{"text": "test"}"#;
        let event = parse_event("", data).unwrap();
        match event {
            StreamEvent::Content(content) => {
                assert_eq!(content.text, "test");
            }
            _ => panic!("Expected Content event"),
        }
    }

    #[test]
    fn test_parse_event_done() {
        let event = parse_event("done", "").unwrap();
        assert!(matches!(event, StreamEvent::Done));
    }

    #[test]
    fn test_parse_event_error() {
        let data = r#"{"message": "Something went wrong", "code": "ERR001"}"#;
        let event = parse_event("error", data).unwrap();
        match event {
            StreamEvent::Error(err) => {
                assert_eq!(err.message, "Something went wrong");
                assert_eq!(err.code, Some("ERR001".to_string()));
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_parse_event_error_no_code() {
        let data = r#"{"message": "Error without code"}"#;
        let event = parse_event("error", data).unwrap();
        match event {
            StreamEvent::Error(err) => {
                assert_eq!(err.message, "Error without code");
                assert_eq!(err.code, None);
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_parse_event_unknown() {
        let event = parse_event("custom_event", "some data").unwrap();
        match event {
            StreamEvent::Unknown { event_type, data } => {
                assert_eq!(event_type, "custom_event");
                assert_eq!(data, "some data");
            }
            _ => panic!("Expected Unknown event"),
        }
    }

    #[test]
    fn test_parse_event_invalid_content_json() {
        let result = parse_event("content", "not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_event_invalid_error_json() {
        let result = parse_event("error", "not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_session_request_serialization() {
        let req = CreateSessionRequest {
            model: Some("claude-3".to_string()),
            system_prompt: Some("You are helpful".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("claude-3"));
        assert!(json.contains("You are helpful"));
    }

    #[test]
    fn test_create_session_request_optional_fields() {
        let req = CreateSessionRequest {
            model: None,
            system_prompt: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("null") || !json.contains("model\":\""));
    }

    #[test]
    fn test_session_deserialization() {
        let json = r#"{"id": "sess_123", "model": "claude-3", "created_at": "2026-01-10T00:00:00Z"}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "sess_123");
        assert_eq!(session.model, "claude-3");
        assert_eq!(session.created_at, "2026-01-10T00:00:00Z");
    }

    #[test]
    fn test_send_message_request_serialization() {
        let req = SendMessageRequest {
            content: "Hello!".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Hello!"));
    }

    #[test]
    fn test_content_event_deserialization() {
        let json = r#"{"text": "Response text"}"#;
        let event: ContentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.text, "Response text");
    }

    #[test]
    fn test_error_event_deserialization() {
        let json = r#"{"message": "Rate limited", "code": "429"}"#;
        let event: ErrorEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.message, "Rate limited");
        assert_eq!(event.code, Some("429".to_string()));
    }

    #[test]
    fn test_stream_event_clone() {
        let content = StreamEvent::Content(ContentEvent {
            text: "test".to_string(),
        });
        let cloned = content.clone();
        match cloned {
            StreamEvent::Content(c) => assert_eq!(c.text, "test"),
            _ => panic!("Clone failed"),
        }

        let done = StreamEvent::Done;
        let done_cloned = done.clone();
        assert!(matches!(done_cloned, StreamEvent::Done));

        let error = StreamEvent::Error(ErrorEvent {
            message: "err".to_string(),
            code: None,
        });
        let error_cloned = error.clone();
        match error_cloned {
            StreamEvent::Error(e) => assert_eq!(e.message, "err"),
            _ => panic!("Clone failed"),
        }

        let unknown = StreamEvent::Unknown {
            event_type: "custom".to_string(),
            data: "data".to_string(),
        };
        let unknown_cloned = unknown.clone();
        match unknown_cloned {
            StreamEvent::Unknown { event_type, data } => {
                assert_eq!(event_type, "custom");
                assert_eq!(data, "data");
            }
            _ => panic!("Clone failed"),
        }
    }

    #[test]
    fn test_stream_event_debug() {
        let content = StreamEvent::Content(ContentEvent {
            text: "test".to_string(),
        });
        let debug_str = format!("{:?}", content);
        assert!(debug_str.contains("Content"));

        let done = StreamEvent::Done;
        let debug_str = format!("{:?}", done);
        assert!(debug_str.contains("Done"));
    }

    #[test]
    fn test_content_event_clone() {
        let event = ContentEvent {
            text: "hello".to_string(),
        };
        let cloned = event.clone();
        assert_eq!(cloned.text, "hello");
    }

    #[test]
    fn test_error_event_clone() {
        let event = ErrorEvent {
            message: "error".to_string(),
            code: Some("500".to_string()),
        };
        let cloned = event.clone();
        assert_eq!(cloned.message, "error");
        assert_eq!(cloned.code, Some("500".to_string()));
    }
}
