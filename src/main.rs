//! Outer.sh server - collaborative AI conversation interface

mod error;
mod models;
mod opencode;
mod store;
mod websocket;

use axum::{routing::get, Router};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::store::Store;

/// Application state shared across handlers
pub struct AppState {
    pub store: Store,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "outer=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Database connection
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:outer.db".into());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    let store = Store::new(pool);
    let state = Arc::new(AppState { store });

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws", get(websocket::handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}
