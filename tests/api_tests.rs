//! API integration tests

use axum::{routing::get, Router};
use outer::AppState;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

async fn setup_app() -> (Router, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Run migrations manually
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
        .route("/health", get(health))
        .route("/ws", get(outer::websocket::handler))
        .with_state(state);

    (app, pool)
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::test]
async fn test_health_endpoint() {
    let (app, _pool) = setup_app().await;

    let response = app
        .oneshot(
            hyper::Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), hyper::StatusCode::OK);
}

#[tokio::test]
async fn test_app_state_new() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    let state = AppState::new(pool);
    // Just verify we can create AppState
    assert!(std::sync::Arc::strong_count(&state) == 1);
}
