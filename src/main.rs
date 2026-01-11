//! Outer.sh server - collaborative AI conversation interface

use std::borrow::Cow;
use std::path::Path;

use axum::{routing::get, Router};
use clap::Parser;
use outer::AppState;
use reedline::{DefaultPrompt, DefaultPromptSegment, Reedline, Signal};
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Outer.sh server - collaborative AI conversation interface
#[derive(Parser, Debug)]
#[command(name = "outer")]
#[command(about = "Outer.sh server - collaborative AI conversation interface")]
#[command(version)]
struct Args {
    /// Database URL (SQLite connection string)
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite:outer.db?mode=rwc")]
    database_url: String,

    /// Port to listen on
    #[arg(short, long, env = "PORT", default_value = "3000")]
    port: u16,

    /// Host to bind to
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: String,

    /// Skip interactive prompts (for automation)
    #[arg(long, env = "OUTER_NON_INTERACTIVE")]
    non_interactive: bool,
}

/// Extract the file path from a SQLite connection URL.
/// Handles formats like:
/// - `sqlite:path.db`
/// - `sqlite://path.db`
/// - `sqlite:path.db?mode=rwc`
fn extract_sqlite_path(url: &str) -> Option<&str> {
    let url = url.strip_prefix("sqlite:")?;
    let url = url.strip_prefix("//").unwrap_or(url);

    // Remove query string if present
    let path = url.split('?').next().unwrap_or(url);

    // :memory: is a special case - no file
    if path == ":memory:" || path.is_empty() {
        return None;
    }

    Some(path)
}

/// Prompt the user about database creation using reedline.
/// Returns the database URL to use, or None if the user wants to exit.
fn prompt_for_database(default_path: &str) -> anyhow::Result<Option<String>> {
    println!("\nDatabase file '{}' does not exist.", default_path);
    println!("Would you like to create it?\n");
    println!("  [Enter]  Create database at '{}'", default_path);
    println!("  [path]   Create database at a different location");
    println!("  [q]      Quit without creating\n");

    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("db path".to_string()),
        DefaultPromptSegment::Empty,
    );

    let mut line_editor = Reedline::create();

    match line_editor.read_line(&prompt)? {
        Signal::Success(input) => {
            let input = input.trim();
            if input.is_empty() {
                // Use default path
                Ok(Some(format!("sqlite:{}?mode=rwc", default_path)))
            } else if input.eq_ignore_ascii_case("q") || input.eq_ignore_ascii_case("quit") {
                Ok(None)
            } else {
                // Use custom path - ensure it has proper extension
                let path = if input.ends_with(".db") || input.ends_with(".sqlite") {
                    Cow::Borrowed(input)
                } else {
                    Cow::Owned(format!("{}.db", input))
                };
                Ok(Some(format!("sqlite:{}?mode=rwc", path)))
            }
        }
        Signal::CtrlC | Signal::CtrlD => Ok(None),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI args first - this handles --help, --version before any other work
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "outer=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Check if database exists and prompt if needed
    let database_url = if let Some(db_path) = extract_sqlite_path(&args.database_url) {
        if !Path::new(db_path).exists() {
            if args.non_interactive {
                // In non-interactive mode, just create at the default location
                tracing::info!("Creating new database at '{}'", db_path);
                args.database_url
            } else {
                // Prompt the user
                match prompt_for_database(db_path)? {
                    Some(url) => {
                        if let Some(new_path) = extract_sqlite_path(&url) {
                            tracing::info!("Creating new database at '{}'", new_path);
                        }
                        url
                    }
                    None => {
                        println!("Exiting.");
                        return Ok(());
                    }
                }
            }
        } else {
            args.database_url
        }
    } else {
        // In-memory or unparseable URL - just use as-is
        args.database_url
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = AppState::new(pool);

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws", get(outer::websocket::handler))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Start server
    let bind_addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}
