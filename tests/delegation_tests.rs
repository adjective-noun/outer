//! Delegation system integration tests
//!
//! Tests symmetric delegation flows between humans and agents.

use axum::{routing::get, Router};
use futures::{SinkExt, StreamExt};
use outer::AppState;
use sqlx::sqlite::SqlitePoolOptions;
use std::net::SocketAddr;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

async fn setup_server() -> (SocketAddr, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Run migrations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS journals (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create journals table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blocks (
            id TEXT PRIMARY KEY NOT NULL,
            journal_id TEXT NOT NULL REFERENCES journals(id),
            block_type TEXT NOT NULL CHECK (block_type IN ('user', 'assistant')),
            content TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'streaming', 'complete', 'error')),
            parent_id TEXT REFERENCES blocks(id),
            forked_from_id TEXT REFERENCES blocks(id),
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create blocks table");

    let state = AppState::new(pool.clone());

    let app = Router::new()
        .route("/ws", get(outer::websocket::handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    (addr, pool)
}

async fn connect_ws(addr: SocketAddr) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{}/ws", addr);
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    ws_stream
}

async fn send_msg(ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, msg: serde_json::Value) {
    ws.send(Message::Text(msg.to_string().into())).await.unwrap();
}

async fn recv_msg(ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>) -> serde_json::Value {
    if let Some(Ok(Message::Text(response))) = ws.next().await {
        serde_json::from_str(&response).unwrap()
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn test_register_participant_user() {
    let (addr, _pool) = setup_server().await;
    let mut ws = connect_ws(addr).await;

    let journal_id = Uuid::new_v4();
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws, msg).await;

    let response = recv_msg(&mut ws).await;
    assert_eq!(response["type"], "participant_registered");
    assert_eq!(response["name"], "Alice");
    assert_eq!(response["kind"], "user");

    // User should have delegate and approve capabilities
    let caps = response["capabilities"].as_array().unwrap();
    assert!(caps.iter().any(|c| c == "delegate"));
    assert!(caps.iter().any(|c| c == "approve"));
}

#[tokio::test]
async fn test_register_participant_agent() {
    let (addr, _pool) = setup_server().await;
    let mut ws = connect_ws(addr).await;

    let journal_id = Uuid::new_v4();
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws, msg).await;

    let response = recv_msg(&mut ws).await;
    assert_eq!(response["type"], "participant_registered");
    assert_eq!(response["name"], "Bot");
    assert_eq!(response["kind"], "agent");

    // Agent should have delegate but not approve
    let caps = response["capabilities"].as_array().unwrap();
    assert!(caps.iter().any(|c| c == "delegate"));
    assert!(!caps.iter().any(|c| c == "approve"));
}

#[tokio::test]
async fn test_human_to_agent_delegation() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice (human)
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let alice_response = recv_msg(&mut ws_alice).await;
    let _alice_id = alice_response["participant_id"].as_str().unwrap();

    // Connect Bot (agent)
    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates to Bot
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Implement feature X",
        "assignee_id": bot_id,
        "priority": "high",
        "requires_approval": true
    });
    send_msg(&mut ws_alice, msg).await;

    let response = recv_msg(&mut ws_alice).await;
    assert_eq!(response["type"], "work_delegated");
    let work_item = &response["work_item"];
    assert_eq!(work_item["description"], "Implement feature X");
    assert_eq!(work_item["priority"], "high");
    assert_eq!(work_item["status"], "pending");
}

#[tokio::test]
async fn test_agent_to_human_delegation() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Bot (agent)
    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let _bot_response = recv_msg(&mut ws_bot).await;

    // Connect Alice (human)
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let alice_response = recv_msg(&mut ws_alice).await;
    let alice_id = alice_response["participant_id"].as_str().unwrap();

    // Bot delegates back to Alice
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Need clarification on requirements",
        "assignee_id": alice_id
    });
    send_msg(&mut ws_bot, msg).await;

    let response = recv_msg(&mut ws_bot).await;
    assert_eq!(response["type"], "work_delegated");
    assert_eq!(response["work_item"]["description"], "Need clarification on requirements");
}

#[tokio::test]
async fn test_accept_and_complete_work() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice (human)
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _alice_response = recv_msg(&mut ws_alice).await;

    // Connect Bot (agent)
    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates to Bot (no approval required)
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Simple task",
        "assignee_id": bot_id,
        "requires_approval": false
    });
    send_msg(&mut ws_alice, msg).await;
    let delegate_response = recv_msg(&mut ws_alice).await;
    let work_item_id = delegate_response["work_item"]["id"].as_str().unwrap();

    // Bot accepts the work
    let msg = serde_json::json!({
        "type": "accept_work",
        "work_item_id": work_item_id
    });
    send_msg(&mut ws_bot, msg).await;
    let accept_response = recv_msg(&mut ws_bot).await;
    assert_eq!(accept_response["type"], "work_accepted");

    // Bot submits the work
    let msg = serde_json::json!({
        "type": "submit_work",
        "work_item_id": work_item_id,
        "result": "Task completed successfully"
    });
    send_msg(&mut ws_bot, msg).await;
    let submit_response = recv_msg(&mut ws_bot).await;

    // Since no approval required, should be directly approved
    assert_eq!(submit_response["type"], "work_approved");
}

#[tokio::test]
async fn test_approval_flow() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice (human)
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _alice_response = recv_msg(&mut ws_alice).await;

    // Connect Bot (agent)
    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates to Bot with approval required
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Important task",
        "assignee_id": bot_id,
        "requires_approval": true
    });
    send_msg(&mut ws_alice, msg).await;
    let delegate_response = recv_msg(&mut ws_alice).await;
    let work_item_id = delegate_response["work_item"]["id"].as_str().unwrap();

    // Bot accepts and submits
    let msg = serde_json::json!({
        "type": "accept_work",
        "work_item_id": work_item_id
    });
    send_msg(&mut ws_bot, msg).await;
    let _ = recv_msg(&mut ws_bot).await;

    let msg = serde_json::json!({
        "type": "submit_work",
        "work_item_id": work_item_id,
        "result": "Work done"
    });
    send_msg(&mut ws_bot, msg).await;
    let submit_response = recv_msg(&mut ws_bot).await;

    // Should be awaiting approval
    assert_eq!(submit_response["type"], "approval_requested");
    let approval_id = submit_response["approval"]["id"].as_str().unwrap();

    // Alice approves
    let msg = serde_json::json!({
        "type": "approve_work",
        "approval_id": approval_id,
        "feedback": "Good job!"
    });
    send_msg(&mut ws_alice, msg).await;
    let approve_response = recv_msg(&mut ws_alice).await;
    assert_eq!(approve_response["type"], "work_approved");
    assert_eq!(approve_response["feedback"], "Good job!");
}

#[tokio::test]
async fn test_decline_work() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice and Bot
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _ = recv_msg(&mut ws_alice).await;

    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Impossible task",
        "assignee_id": bot_id
    });
    send_msg(&mut ws_alice, msg).await;
    let delegate_response = recv_msg(&mut ws_alice).await;
    let work_item_id = delegate_response["work_item"]["id"].as_str().unwrap();

    // Bot declines
    let msg = serde_json::json!({
        "type": "decline_work",
        "work_item_id": work_item_id
    });
    send_msg(&mut ws_bot, msg).await;
    let decline_response = recv_msg(&mut ws_bot).await;
    assert_eq!(decline_response["type"], "work_declined");
}

#[tokio::test]
async fn test_get_work_queue() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice and Bot
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _ = recv_msg(&mut ws_alice).await;

    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates multiple tasks
    for i in 1..=3 {
        let msg = serde_json::json!({
            "type": "delegate",
            "journal_id": journal_id.to_string(),
            "description": format!("Task {}", i),
            "assignee_id": bot_id
        });
        send_msg(&mut ws_alice, msg).await;
        let _ = recv_msg(&mut ws_alice).await;
    }

    // Bot gets work queue
    let msg = serde_json::json!({
        "type": "get_work_queue"
    });
    send_msg(&mut ws_bot, msg).await;
    let queue_response = recv_msg(&mut ws_bot).await;
    assert_eq!(queue_response["type"], "work_queue");
    assert_eq!(queue_response["items"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_cancel_work() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice and Bot
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _ = recv_msg(&mut ws_alice).await;

    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Task to cancel",
        "assignee_id": bot_id
    });
    send_msg(&mut ws_alice, msg).await;
    let delegate_response = recv_msg(&mut ws_alice).await;
    let work_item_id = delegate_response["work_item"]["id"].as_str().unwrap();

    // Alice cancels
    let msg = serde_json::json!({
        "type": "cancel_work",
        "work_item_id": work_item_id
    });
    send_msg(&mut ws_alice, msg).await;
    let cancel_response = recv_msg(&mut ws_alice).await;
    assert_eq!(cancel_response["type"], "work_cancelled");
}

#[tokio::test]
async fn test_human_to_human_delegation() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _ = recv_msg(&mut ws_alice).await;

    // Connect Bob
    let mut ws_bob = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bob",
        "kind": "user"
    });
    send_msg(&mut ws_bob, msg).await;
    let bob_response = recv_msg(&mut ws_bob).await;
    let bob_id = bob_response["participant_id"].as_str().unwrap();

    // Alice delegates to Bob
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Review my code",
        "assignee_id": bob_id,
        "requires_approval": true
    });
    send_msg(&mut ws_alice, msg).await;
    let response = recv_msg(&mut ws_alice).await;
    assert_eq!(response["type"], "work_delegated");
    assert_eq!(response["work_item"]["description"], "Review my code");
}

#[tokio::test]
async fn test_agent_to_agent_delegation() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Bot1
    let mut ws_bot1 = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot1",
        "kind": "agent"
    });
    send_msg(&mut ws_bot1, msg).await;
    let _ = recv_msg(&mut ws_bot1).await;

    // Connect Bot2
    let mut ws_bot2 = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot2",
        "kind": "agent"
    });
    send_msg(&mut ws_bot2, msg).await;
    let bot2_response = recv_msg(&mut ws_bot2).await;
    let bot2_id = bot2_response["participant_id"].as_str().unwrap();

    // Bot1 delegates to Bot2
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Subtask for specialized agent",
        "assignee_id": bot2_id
    });
    send_msg(&mut ws_bot1, msg).await;
    let response = recv_msg(&mut ws_bot1).await;
    assert_eq!(response["type"], "work_delegated");
    assert_eq!(response["work_item"]["description"], "Subtask for specialized agent");
}

#[tokio::test]
async fn test_get_participants() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect multiple participants
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _ = recv_msg(&mut ws_alice).await;

    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let _ = recv_msg(&mut ws_bot).await;

    // Get available participants
    let msg = serde_json::json!({
        "type": "get_participants",
        "journal_id": journal_id.to_string()
    });
    send_msg(&mut ws_alice, msg).await;
    let response = recv_msg(&mut ws_alice).await;
    assert_eq!(response["type"], "available_participants");
    assert_eq!(response["participants"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_set_accepting_work() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let _ = recv_msg(&mut ws_bot).await;

    // Disable accepting work
    let msg = serde_json::json!({
        "type": "set_accepting_work",
        "accepting": false
    });
    send_msg(&mut ws_bot, msg).await;
    let response = recv_msg(&mut ws_bot).await;
    assert_eq!(response["type"], "accepting_work_changed");
    assert_eq!(response["accepting"], false);
}

#[tokio::test]
async fn test_reject_work() {
    let (addr, _pool) = setup_server().await;
    let journal_id = Uuid::new_v4();

    // Connect Alice and Bot
    let mut ws_alice = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Alice",
        "kind": "user"
    });
    send_msg(&mut ws_alice, msg).await;
    let _ = recv_msg(&mut ws_alice).await;

    let mut ws_bot = connect_ws(addr).await;
    let msg = serde_json::json!({
        "type": "register_participant",
        "journal_id": journal_id.to_string(),
        "name": "Bot",
        "kind": "agent"
    });
    send_msg(&mut ws_bot, msg).await;
    let bot_response = recv_msg(&mut ws_bot).await;
    let bot_id = bot_response["participant_id"].as_str().unwrap();

    // Alice delegates with approval required
    let msg = serde_json::json!({
        "type": "delegate",
        "journal_id": journal_id.to_string(),
        "description": "Needs work",
        "assignee_id": bot_id,
        "requires_approval": true
    });
    send_msg(&mut ws_alice, msg).await;
    let delegate_response = recv_msg(&mut ws_alice).await;
    let work_item_id = delegate_response["work_item"]["id"].as_str().unwrap();

    // Bot accepts and submits
    let msg = serde_json::json!({
        "type": "accept_work",
        "work_item_id": work_item_id
    });
    send_msg(&mut ws_bot, msg).await;
    let _ = recv_msg(&mut ws_bot).await;

    let msg = serde_json::json!({
        "type": "submit_work",
        "work_item_id": work_item_id,
        "result": "First attempt"
    });
    send_msg(&mut ws_bot, msg).await;
    let submit_response = recv_msg(&mut ws_bot).await;
    let approval_id = submit_response["approval"]["id"].as_str().unwrap();

    // Alice rejects
    let msg = serde_json::json!({
        "type": "reject_work",
        "approval_id": approval_id,
        "feedback": "Please add more tests"
    });
    send_msg(&mut ws_alice, msg).await;
    let reject_response = recv_msg(&mut ws_alice).await;
    assert_eq!(reject_response["type"], "work_rejected");
    assert_eq!(reject_response["feedback"], "Please add more tests");
}
