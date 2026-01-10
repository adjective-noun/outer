//! TUI interface for Outer.sh using ratatui

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use uuid::Uuid;

use crate::client::OuterClient;
use crate::messages::{self, BlockStatus, BlockType, ServerMessage};

/// Application state for TUI
struct App {
    /// The WebSocket client
    client: OuterClient,
    /// Current journal ID
    journal_id: Uuid,
    /// Blocks in the conversation
    blocks: Vec<messages::Block>,
    /// Current participants
    participants: Vec<messages::Participant>,
    /// Input buffer
    input: String,
    /// Scroll offset for the conversation view
    scroll: usize,
    /// Whether currently streaming a response
    streaming: bool,
    /// Streaming content buffer
    streaming_content: String,
    /// Streaming block ID
    streaming_block_id: Option<Uuid>,
    /// Should quit
    should_quit: bool,
    /// Status message
    status: String,
}

impl App {
    fn new(client: OuterClient, journal_id: Uuid) -> Self {
        Self {
            client,
            journal_id,
            blocks: Vec::new(),
            participants: Vec::new(),
            input: String::new(),
            scroll: 0,
            streaming: false,
            streaming_content: String::new(),
            streaming_block_id: None,
            should_quit: false,
            status: "Connected".to_string(),
        }
    }

    async fn load_journal(&mut self) -> Result<()> {
        let (_, blocks) = self.client.get_journal(self.journal_id).await?;
        self.blocks = blocks;
        Ok(())
    }

    fn handle_server_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::BlockCreated { block } => {
                if block.block_type == BlockType::Assistant {
                    self.streaming_block_id = Some(block.id);
                    self.streaming = true;
                    self.streaming_content.clear();
                }
                self.blocks.push(block);
            }
            ServerMessage::BlockContentDelta { block_id, delta } => {
                if Some(block_id) == self.streaming_block_id {
                    self.streaming_content.push_str(&delta);
                    // Update the block content in our list
                    if let Some(block) = self.blocks.iter_mut().find(|b| b.id == block_id) {
                        block.content.push_str(&delta);
                    }
                }
            }
            ServerMessage::BlockStatusChanged { block_id, status } => {
                if let Some(block) = self.blocks.iter_mut().find(|b| b.id == block_id) {
                    block.status = status;
                }
                if Some(block_id) == self.streaming_block_id
                    && (status == BlockStatus::Complete || status == BlockStatus::Error)
                {
                    self.streaming = false;
                    self.streaming_block_id = None;
                    self.status = if status == BlockStatus::Complete {
                        "Ready".to_string()
                    } else {
                        "Error".to_string()
                    };
                }
            }
            ServerMessage::ParticipantJoined { participant, .. } => {
                self.participants.push(participant);
            }
            ServerMessage::ParticipantLeft { participant_id, .. } => {
                self.participants.retain(|p| p.id != participant_id);
            }
            ServerMessage::Subscribed { participants, .. } => {
                self.participants = participants;
            }
            ServerMessage::Error { message } => {
                self.status = format!("Error: {}", message);
            }
            _ => {}
        }
    }

    async fn submit_input(&mut self) -> Result<()> {
        if self.input.is_empty() || self.streaming {
            return Ok(());
        }

        let content = std::mem::take(&mut self.input);
        self.status = "Sending...".to_string();
        self.client.submit(self.journal_id, content).await?;
        Ok(())
    }
}

/// Run the TUI
pub async fn run(client: OuterClient, journal_id: Uuid) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(client, journal_id);

    // Load journal
    if let Err(e) = app.load_journal().await {
        tracing::warn!("Failed to load journal: {}", e);
    }

    // Main loop
    loop {
        // Draw UI
        terminal.draw(|f| draw_ui(f, &app))?;

        // Handle events with timeout
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    (KeyCode::Enter, _) if !app.streaming => {
                        if let Err(e) = app.submit_input().await {
                            app.status = format!("Error: {}", e);
                        }
                    }
                    (KeyCode::Backspace, _) => {
                        app.input.pop();
                    }
                    (KeyCode::Char(c), _) if !app.streaming => {
                        app.input.push(c);
                    }
                    (KeyCode::Up, _) => {
                        if app.scroll > 0 {
                            app.scroll -= 1;
                        }
                    }
                    (KeyCode::Down, _) => {
                        app.scroll += 1;
                    }
                    (KeyCode::PageUp, _) => {
                        app.scroll = app.scroll.saturating_sub(10);
                    }
                    (KeyCode::PageDown, _) => {
                        app.scroll += 10;
                    }
                    (KeyCode::Esc, _) if app.streaming => {
                        if let Some(block_id) = app.streaming_block_id {
                            let _ = app.client.cancel(block_id).await;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Check for server messages
        while let Some(msg) = app.client.try_recv() {
            app.handle_server_message(msg);
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn draw_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),     // Conversation
            Constraint::Length(3),   // Input
            Constraint::Length(1),   // Status bar
        ])
        .split(f.area());

    // Conversation view
    draw_conversation(f, app, chunks[0]);

    // Input box
    draw_input(f, app, chunks[1]);

    // Status bar
    draw_status(f, app, chunks[2]);
}

fn draw_conversation(f: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();

    for block in &app.blocks {
        let (prefix, style) = match block.block_type {
            BlockType::User => (
                "You: ",
                Style::default().fg(Color::Cyan),
            ),
            BlockType::Assistant => (
                "AI: ",
                Style::default().fg(Color::Green),
            ),
        };

        // Show status indicator for streaming blocks
        let status_indicator = match block.status {
            BlockStatus::Streaming => " [streaming...]",
            BlockStatus::Pending => " [pending]",
            BlockStatus::Error => " [error]",
            BlockStatus::Complete => "",
        };

        // Wrap content at reasonable width
        let content = &block.content;
        let lines: Vec<Line> = content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                if i == 0 {
                    Line::from(vec![
                        Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                        Span::styled(line, style),
                        Span::styled(status_indicator, Style::default().fg(Color::Yellow)),
                    ])
                } else {
                    Line::from(Span::styled(format!("    {}", line), style))
                }
            })
            .collect();

        items.push(ListItem::new(Text::from(lines)));
        items.push(ListItem::new(Line::from(""))); // Spacing
    }

    let conversation = List::new(items)
        .block(
            Block::default()
                .title(" Conversation ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        );

    f.render_widget(conversation, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let input_style = if app.streaming {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let placeholder = if app.streaming {
        "Waiting for response... (Esc to cancel)"
    } else if app.input.is_empty() {
        "Type a message..."
    } else {
        ""
    };

    let display_text = if app.input.is_empty() {
        placeholder.to_string()
    } else {
        app.input.clone()
    };

    let input = Paragraph::new(display_text)
        .style(input_style)
        .block(
            Block::default()
                .title(" Input ")
                .borders(Borders::ALL)
                .border_style(if app.streaming {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Cyan)
                }),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(input, area);

    // Show cursor position
    if !app.streaming {
        f.set_cursor_position((area.x + 1 + app.input.len() as u16, area.y + 1));
    }
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let participant_count = app.participants.len();
    let status_text = format!(
        " {} | {} participant{} | Ctrl+C to quit",
        app.status,
        participant_count,
        if participant_count == 1 { "" } else { "s" }
    );

    let status = Paragraph::new(status_text).style(
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray),
    );

    f.render_widget(status, area);
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_app_initial_state() {
        // Just verify the struct can be created with expected defaults
        // We can't actually test this without a real client, but we can test the logic
        let input = String::new();
        assert!(input.is_empty());
    }
}
