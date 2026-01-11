//! Outer CLI client - TUI for collaborative AI conversations

mod client;
mod messages;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "outer")]
#[command(about = "CLI client for Outer.sh - collaborative AI conversation interface")]
#[command(version)]
struct Cli {
    /// Server URL (default: ws://localhost:3000/ws)
    #[arg(short, long, default_value = "ws://localhost:3000/ws")]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to a journal and start interactive TUI
    Connect {
        /// Journal ID to connect to
        #[arg(short, long)]
        journal: Option<String>,

        /// Create a new journal if not specified
        #[arg(short, long)]
        new: bool,

        /// Your display name
        #[arg(short = 'n', long, default_value = "CLI User")]
        name: String,
    },

    /// List all journals
    List,

    /// Submit a message to a journal (non-interactive)
    Submit {
        /// Journal ID
        #[arg(short, long)]
        journal: String,

        /// Message content
        #[arg(short, long)]
        message: String,
    },

    /// Fork a block to create a new branch
    Fork {
        /// Block ID to fork
        #[arg(short, long)]
        block: String,
    },

    /// Run in agent/headless mode
    Agent {
        /// Journal ID to connect to
        #[arg(short, long)]
        journal: String,

        /// Agent name
        #[arg(short = 'n', long, default_value = "Agent")]
        name: String,

        /// Exit after receiving first response
        #[arg(long)]
        once: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "outer_cli=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Connect { journal, new, name } => {
            run_connect(&cli.server, journal, new, &name).await
        }
        Commands::List => run_list(&cli.server).await,
        Commands::Submit { journal, message } => run_submit(&cli.server, &journal, &message).await,
        Commands::Fork { block } => run_fork(&cli.server, &block).await,
        Commands::Agent {
            journal,
            name,
            once,
        } => run_agent(&cli.server, &journal, &name, once).await,
    }
}

async fn run_connect(
    server: &str,
    journal_id: Option<String>,
    new: bool,
    name: &str,
) -> Result<()> {
    let mut client = client::OuterClient::connect(server).await?;

    // Get or create journal
    let journal_id = if new || journal_id.is_none() {
        // Create new journal
        let title = if new {
            Some("CLI Session".to_string())
        } else {
            None
        };
        let journal = client.create_journal(title).await?;
        tracing::info!("Created journal: {}", journal.id);
        journal.id
    } else {
        journal_id.unwrap().parse()?
    };

    // Subscribe to journal
    client.subscribe(journal_id, name.to_string(), None).await?;

    // Run TUI
    tui::run(client, journal_id).await
}

async fn run_list(server: &str) -> Result<()> {
    let mut client = client::OuterClient::connect(server).await?;
    let journals = client.list_journals().await?;

    if journals.is_empty() {
        println!("No journals found.");
    } else {
        println!("Journals:");
        println!("{:─<60}", "");
        for journal in journals {
            println!(
                "  {} - {} (updated: {})",
                journal.id,
                journal.title,
                journal.updated_at.format("%Y-%m-%d %H:%M")
            );
        }
    }

    Ok(())
}

async fn run_submit(server: &str, journal_id: &str, message: &str) -> Result<()> {
    let mut client = client::OuterClient::connect(server).await?;
    let journal_id: uuid::Uuid = journal_id.parse()?;

    println!("Submitting message to journal {}...", journal_id);

    // Submit and stream response
    client
        .submit_and_stream(journal_id, message.to_string(), |event| match event {
            messages::ServerMessage::BlockContentDelta { delta, .. } => {
                print!("{}", delta);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            messages::ServerMessage::BlockStatusChanged { status, .. } => {
                if status == messages::BlockStatus::Complete {
                    println!();
                }
            }
            _ => {}
        })
        .await?;

    Ok(())
}

async fn run_fork(server: &str, block_id: &str) -> Result<()> {
    let mut client = client::OuterClient::connect(server).await?;
    let block_id: uuid::Uuid = block_id.parse()?;

    println!("Forking block {}...", block_id);

    client
        .fork_and_stream(block_id, |event| match event {
            messages::ServerMessage::BlockForked { new_block, .. } => {
                println!("Created fork: {}", new_block.id);
            }
            messages::ServerMessage::BlockContentDelta { delta, .. } => {
                print!("{}", delta);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            messages::ServerMessage::BlockStatusChanged { status, .. } => {
                if status == messages::BlockStatus::Complete {
                    println!();
                }
            }
            _ => {}
        })
        .await?;

    Ok(())
}

async fn run_agent(server: &str, journal_id: &str, name: &str, once: bool) -> Result<()> {
    let mut client = client::OuterClient::connect(server).await?;
    let journal_id: uuid::Uuid = journal_id.parse()?;

    // Subscribe as agent
    client
        .subscribe(journal_id, name.to_string(), Some("agent".to_string()))
        .await?;

    // In agent mode, we listen for events and can respond
    println!("Agent {} connected to journal {}", name, journal_id);
    println!("Listening for events...");

    client
        .listen(|event| {
            match &event {
                messages::ServerMessage::BlockCreated { block } => {
                    if block.block_type == messages::BlockType::User {
                        println!("\n[User] {}", block.content);
                    }
                }
                messages::ServerMessage::BlockContentDelta { delta, .. } => {
                    print!("{}", delta);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
                messages::ServerMessage::BlockStatusChanged { status, .. } => {
                    if *status == messages::BlockStatus::Complete {
                        println!();
                        if once {
                            return false; // Stop listening
                        }
                    }
                }
                _ => {}
            }
            true // Continue listening
        })
        .await?;

    Ok(())
}
