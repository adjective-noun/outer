//! Integration tests for OpenCode client

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

#[tokio::test]
async fn test_create_session_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "sess_123",
            "model": "claude-3",
            "created_at": "2026-01-10T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let session = client
        .create_session(outer::opencode::CreateSessionRequest {
            model: Some("claude-3".to_string()),
            system_prompt: None,
        })
        .await
        .unwrap();

    assert_eq!(session.id, "sess_123");
    assert_eq!(session.model, "claude-3");
}

#[tokio::test]
async fn test_create_session_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let result = client
        .create_session(outer::opencode::CreateSessionRequest {
            model: None,
            system_prompt: None,
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_send_message_success() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions/sess_123/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("event: content\ndata: {\"text\": \"Hello\"}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "sess_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let first = stream.next().await.unwrap().unwrap();
    match first {
        outer::opencode::StreamEvent::Content(content) => {
            assert_eq!(content.text, "Hello");
        }
        _ => panic!("Expected Content event"),
    }
}

#[tokio::test]
async fn test_send_message_error_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions/sess_123/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let result = client
        .send_message(
            "sess_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_subscribe_events_success() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/sessions/sess_123/events"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("event: content\ndata: {\"text\": \"Event 1\"}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client.subscribe_events("sess_123").await.unwrap();

    let event = stream.next().await.unwrap().unwrap();
    match event {
        outer::opencode::StreamEvent::Content(content) => {
            assert_eq!(content.text, "Event 1");
        }
        _ => panic!("Expected Content event"),
    }
}

#[tokio::test]
async fn test_subscribe_events_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/sessions/sess_123/events"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let result = client.subscribe_events("sess_123").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_stream_error_event() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions/sess_123/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    "event: error\ndata: {\"message\": \"Rate limited\", \"code\": \"429\"}\n\n",
                )
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "sess_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let event = stream.next().await.unwrap().unwrap();
    match event {
        outer::opencode::StreamEvent::Error(err) => {
            assert_eq!(err.message, "Rate limited");
            assert_eq!(err.code, Some("429".to_string()));
        }
        _ => panic!("Expected Error event"),
    }
}

#[tokio::test]
async fn test_stream_unknown_event() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions/sess_123/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("event: custom\ndata: custom_data\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "sess_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let event = stream.next().await.unwrap().unwrap();
    match event {
        outer::opencode::StreamEvent::Unknown { event_type, data } => {
            assert_eq!(event_type, "custom");
            assert_eq!(data, "custom_data");
        }
        _ => panic!("Expected Unknown event"),
    }
}

#[tokio::test]
async fn test_stream_multiline_data() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/sessions/sess_123/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("event: content\ndata: {\"text\": \"Line 1\"}\n\nevent: content\ndata: {\"text\": \"Line 2\"}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "sess_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let first = stream.next().await.unwrap().unwrap();
    match first {
        outer::opencode::StreamEvent::Content(content) => {
            assert_eq!(content.text, "Line 1");
        }
        _ => panic!("Expected Content event"),
    }

    let second = stream.next().await.unwrap().unwrap();
    match second {
        outer::opencode::StreamEvent::Content(content) => {
            assert_eq!(content.text, "Line 2");
        }
        _ => panic!("Expected Content event"),
    }
}
