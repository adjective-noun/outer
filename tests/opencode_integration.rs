//! Integration tests for OpenCode client

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

#[tokio::test]
async fn test_create_session_success() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "ses_123",
            "version": "1.0.0",
            "projectID": "proj_456"
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

    assert_eq!(session.id, "ses_123");
}

#[tokio::test]
async fn test_create_session_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/session"))
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

    // Mock the event stream endpoint
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {\"type\": \"message.part.updated\", \"properties\": {\"delta\": \"Hello\", \"part\": {\"sessionID\": \"ses_123\"}}}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    // Mock the prompt_async endpoint
    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "ses_123",
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
        _ => panic!("Expected Content event, got {:?}", first),
    }
}

#[tokio::test]
async fn test_send_message_error_response() {
    let mock_server = MockServer::start().await;

    // Mock the event stream endpoint first (it's called before prompt)
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    // Mock the prompt_async endpoint with error
    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(400).set_body_string("Bad Request"))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let result = client
        .send_message(
            "ses_123",
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
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {\"type\": \"message.part.updated\", \"properties\": {\"delta\": \"Event 1\", \"part\": {\"sessionID\": \"ses_123\"}}}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client.subscribe_events("ses_123").await.unwrap();

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
        .and(path("/event"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let result = client.subscribe_events("ses_123").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_stream_error_event() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    // Mock the event stream endpoint
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {\"type\": \"session.error\", \"properties\": {\"sessionID\": \"ses_123\", \"error\": {\"message\": \"Rate limited\"}}}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    // Mock the prompt_async endpoint
    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "ses_123",
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
        }
        _ => panic!("Expected Error event"),
    }
}

#[tokio::test]
async fn test_stream_session_idle() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    // Mock the event stream endpoint
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {\"type\": \"session.idle\", \"properties\": {\"sessionID\": \"ses_123\"}}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    // Mock the prompt_async endpoint
    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "ses_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, outer::opencode::StreamEvent::Done));
}

#[tokio::test]
async fn test_stream_filters_other_sessions() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    // Event for a different session should be filtered out
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    "data: {\"type\": \"session.idle\", \"properties\": {\"sessionID\": \"ses_other\"}}\n\ndata: {\"type\": \"session.idle\", \"properties\": {\"sessionID\": \"ses_123\"}}\n\n"
                )
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    // Mock the prompt_async endpoint
    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "ses_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    // First event for ses_other should be filtered, we should get ses_123's event
    let event = stream.next().await.unwrap().unwrap();
    assert!(matches!(event, outer::opencode::StreamEvent::Done));
}

#[tokio::test]
async fn test_stream_unknown_event() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: {\"type\": \"custom_event\", \"properties\": {}}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "ses_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let event = stream.next().await.unwrap().unwrap();
    match event {
        outer::opencode::StreamEvent::Unknown { event_type, .. } => {
            assert_eq!(event_type, "custom_event");
        }
        _ => panic!("Expected Unknown event"),
    }
}

#[tokio::test]
async fn test_stream_multiple_content_deltas() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(
                    "data: {\"type\": \"message.part.updated\", \"properties\": {\"delta\": \"Hello \", \"part\": {\"sessionID\": \"ses_123\"}}}\n\ndata: {\"type\": \"message.part.updated\", \"properties\": {\"delta\": \"World!\", \"part\": {\"sessionID\": \"ses_123\"}}}\n\n"
                )
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/session/ses_123/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock_server)
        .await;

    let client = outer::opencode::OpenCodeClient::new(&mock_server.uri());
    let mut stream = client
        .send_message(
            "ses_123",
            outer::opencode::SendMessageRequest {
                content: "Hi".to_string(),
            },
        )
        .await
        .unwrap();

    let first = stream.next().await.unwrap().unwrap();
    match first {
        outer::opencode::StreamEvent::Content(content) => {
            assert_eq!(content.text, "Hello ");
        }
        _ => panic!("Expected Content event"),
    }

    let second = stream.next().await.unwrap().unwrap();
    match second {
        outer::opencode::StreamEvent::Content(content) => {
            assert_eq!(content.text, "World!");
        }
        _ => panic!("Expected Content event"),
    }
}
