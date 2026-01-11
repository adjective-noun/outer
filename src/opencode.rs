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
    pub async fn create_session(&self, _request: CreateSessionRequest) -> Result<Session> {
        let response = self
            .client
            .post(format!("{}/session", self.base_url))
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    tracing::warn!(
                        "Failed to connect to OpenCode server at {}: {}",
                        self.base_url,
                        e
                    );
                }
                AppError::OpenCode(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to create session: {} - {}",
                status, text
            )));
        }

        let text = response
            .text()
            .await
            .map_err(|e| AppError::OpenCode(format!("Failed to read response: {}", e)))?;

        serde_json::from_str(&text).map_err(|e| {
            // Log the full response for debugging
            tracing::error!(
                "Failed to parse OpenCode response as JSON: {}. Full response:\n{}",
                e,
                text
            );
            // Send truncated preview to client
            let preview = if text.len() > 200 {
                format!("{}...", &text[..200])
            } else {
                text.clone()
            };
            AppError::OpenCode(format!(
                "Invalid JSON from OpenCode: {}. Response: {}",
                e, preview
            ))
        })
    }

    /// Send a message and stream the response
    pub async fn send_message(
        &self,
        session_id: &str,
        request: SendMessageRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        // First, subscribe to the event stream
        let event_response = self
            .client
            .get(format!("{}/event", self.base_url))
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    tracing::warn!(
                        "Failed to connect to OpenCode server at {}: {}",
                        self.base_url,
                        e
                    );
                }
                AppError::OpenCode(e.to_string())
            })?;

        if !event_response.status().is_success() {
            let status = event_response.status();
            let text = event_response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to subscribe to events: {} - {}",
                status, text
            )));
        }

        // Send the prompt asynchronously
        let prompt_body = PromptRequest {
            parts: vec![TextPart {
                r#type: "text".to_string(),
                text: request.content,
            }],
        };

        let prompt_response = self
            .client
            .post(format!(
                "{}/session/{}/prompt_async",
                self.base_url, session_id
            ))
            .json(&prompt_body)
            .send()
            .await
            .map_err(|e| AppError::OpenCode(format!("Failed to send prompt: {}", e)))?;

        // prompt_async returns 204 on success
        if !prompt_response.status().is_success() {
            let status = prompt_response.status();
            let text = prompt_response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to send message: {} - {}",
                status, text
            )));
        }

        // Parse SSE stream, filtering for our session
        let session_id_owned = session_id.to_string();
        let stream = parse_sse_stream(event_response, Some(session_id_owned));
        Ok(Box::pin(stream))
    }

    /// Subscribe to events for a session
    pub async fn subscribe_events(
        &self,
        session_id: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let response = self
            .client
            .get(format!("{}/event", self.base_url))
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    tracing::warn!(
                        "Failed to connect to OpenCode server at {}: {}",
                        self.base_url,
                        e
                    );
                }
                AppError::OpenCode(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::OpenCode(format!(
                "Failed to subscribe to events: {} - {}",
                status, text
            )));
        }

        let session_id_owned = session_id.to_string();
        let stream = parse_sse_stream(response, Some(session_id_owned));
        Ok(Box::pin(stream))
    }
}

/// Parse SSE stream from response
fn parse_sse_stream(
    response: reqwest::Response,
    session_filter: Option<String>,
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
                        match parse_event(&event_type, &data, session_filter.as_deref()) {
                            Ok(Some(event)) => yield Ok(event),
                            Ok(None) => {} // Event filtered out
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

/// Parse an OpenCode event into our StreamEvent type
pub(crate) fn parse_event(
    event_type: &str,
    data: &str,
    session_filter: Option<&str>,
) -> Result<Option<StreamEvent>> {
    // Parse the event JSON
    let event: serde_json::Value = serde_json::from_str(data).map_err(|e| {
        tracing::error!("Failed to parse event as JSON: {}. Full data:\n{}", e, data);
        AppError::OpenCode(format!("Failed to parse event: {}", e))
    })?;

    // Get the event type from the payload
    let payload_type = event
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or(event_type);
    let properties = event.get("properties");

    // Filter by session ID if specified
    if let Some(filter_session) = session_filter {
        if let Some(props) = properties {
            if let Some(session_id) = props.get("sessionID").and_then(|s| s.as_str()) {
                if session_id != filter_session {
                    return Ok(None); // Skip events for other sessions
                }
            }
            // Also check for part.sessionID
            if let Some(part) = props.get("part") {
                if let Some(session_id) = part.get("sessionID").and_then(|s| s.as_str()) {
                    if session_id != filter_session {
                        return Ok(None);
                    }
                }
            }
        }
    }

    match payload_type {
        "message.part.updated" => {
            if let Some(props) = properties {
                // Check if there's a delta (streaming text)
                if let Some(delta) = props.get("delta").and_then(|d| d.as_str()) {
                    return Ok(Some(StreamEvent::Content(ContentEvent {
                        text: delta.to_string(),
                    })));
                }
                // Check for text content in the part itself
                if let Some(part) = props.get("part") {
                    if let Some(content) = part.get("content").and_then(|c| c.as_str()) {
                        // Only emit if this looks like new content
                        if !content.is_empty() {
                            return Ok(Some(StreamEvent::Content(ContentEvent {
                                text: content.to_string(),
                            })));
                        }
                    }
                }
            }
            Ok(None)
        }
        "session.idle" => Ok(Some(StreamEvent::Done)),
        "session.error" => {
            if let Some(props) = properties {
                let error_msg = if let Some(error) = props.get("error") {
                    if let Some(msg) = error.get("message").and_then(|m| m.as_str()) {
                        msg.to_string()
                    } else {
                        error.to_string()
                    }
                } else {
                    "Unknown error".to_string()
                };
                Ok(Some(StreamEvent::Error(ErrorEvent {
                    message: error_msg,
                    code: None,
                })))
            } else {
                Ok(Some(StreamEvent::Error(ErrorEvent {
                    message: "Session error".to_string(),
                    code: None,
                })))
            }
        }
        _ => {
            // Unknown/unhandled event type
            Ok(Some(StreamEvent::Unknown {
                event_type: payload_type.to_string(),
                data: data.to_string(),
            }))
        }
    }
}

// Request/Response types

#[derive(Debug, Serialize)]
pub struct CreateSessionRequest {
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
struct PromptRequest {
    parts: Vec<TextPart>,
}

#[derive(Debug, Serialize)]
struct TextPart {
    r#type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Session {
    pub id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "projectID")]
    pub project_id: Option<String>,
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
    fn test_parse_event_message_part_with_delta() {
        let data = r#"{"type": "message.part.updated", "properties": {"delta": "Hello!", "part": {"sessionID": "ses_123"}}}"#;
        let event = parse_event("", data, Some("ses_123")).unwrap();
        match event {
            Some(StreamEvent::Content(content)) => {
                assert_eq!(content.text, "Hello!");
            }
            _ => panic!("Expected Content event"),
        }
    }

    #[test]
    fn test_parse_event_session_idle() {
        let data = r#"{"type": "session.idle", "properties": {"sessionID": "ses_123"}}"#;
        let event = parse_event("", data, Some("ses_123")).unwrap();
        assert!(matches!(event, Some(StreamEvent::Done)));
    }

    #[test]
    fn test_parse_event_session_error() {
        let data = r#"{"type": "session.error", "properties": {"sessionID": "ses_123", "error": {"message": "Rate limited"}}}"#;
        let event = parse_event("", data, Some("ses_123")).unwrap();
        match event {
            Some(StreamEvent::Error(err)) => {
                assert_eq!(err.message, "Rate limited");
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_parse_event_filters_other_sessions() {
        let data = r#"{"type": "session.idle", "properties": {"sessionID": "ses_other"}}"#;
        let event = parse_event("", data, Some("ses_123")).unwrap();
        assert!(event.is_none());
    }

    #[test]
    fn test_parse_event_unknown() {
        let data = r#"{"type": "custom_event", "properties": {}}"#;
        let event = parse_event("", data, None).unwrap();
        match event {
            Some(StreamEvent::Unknown { event_type, .. }) => {
                assert_eq!(event_type, "custom_event");
            }
            _ => panic!("Expected Unknown event"),
        }
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
    fn test_session_deserialization() {
        let json = r#"{"id": "ses_123", "version": "1.0", "projectID": "proj_456"}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "ses_123");
        assert_eq!(session.version, Some("1.0".to_string()));
        assert_eq!(session.project_id, Some("proj_456".to_string()));
    }

    #[test]
    fn test_session_deserialization_minimal() {
        let json = r#"{"id": "ses_123"}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "ses_123");
        assert_eq!(session.version, None);
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
