# outer

A collaborative AI conversation interface server. Outer provides real-time WebSocket communication for managing conversation journals with streaming AI responses.

## Prerequisites

- Rust 1.70+ (with cargo)
- SQLite 3.x
- An OpenCode-compatible backend server

## Installation

Clone and build the project:

```bash
cargo build --release
```

The compiled binary will be at `target/release/outer`.

## Configuration

Outer uses environment variables for configuration:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:outer.db` | SQLite database connection string |
| `OPENCODE_URL` | `http://localhost:8080` | URL of the OpenCode backend server |
| `RUST_LOG` | `outer=debug,tower_http=debug` | Logging configuration |

## Running the Server

```bash
# Development
cargo run

# Production
DATABASE_URL=sqlite:production.db OPENCODE_URL=http://opencode:8080 ./target/release/outer
```

The server listens on `0.0.0.0:3000` by default.

### Health Check

```bash
curl http://localhost:3000/health
# Returns: ok
```

## API Overview

Outer exposes a WebSocket endpoint at `/ws` for real-time communication.

### WebSocket Protocol

Connect to `ws://localhost:3000/ws` and exchange JSON messages.

#### Client Messages

**Create Journal**
```json
{"type": "create_journal", "title": "My Conversation"}
```

**List Journals**
```json
{"type": "list_journals"}
```

**Get Journal**
```json
{"type": "get_journal", "journal_id": "uuid-here"}
```

**Submit Message**
```json
{
  "type": "submit",
  "journal_id": "uuid-here",
  "content": "Hello, AI!",
  "session_id": "optional-opencode-session-id"
}
```

#### Server Messages

**Journal Created**
```json
{"type": "journal_created", "journal_id": "uuid", "title": "My Conversation"}
```

**Journals List**
```json
{"type": "journals", "journals": [...]}
```

**Journal with Blocks**
```json
{"type": "journal", "journal": {...}, "blocks": [...]}
```

**Block Created**
```json
{"type": "block_created", "block": {...}}
```

**Block Content Delta** (streaming)
```json
{"type": "block_content_delta", "block_id": "uuid", "delta": "text chunk"}
```

**Block Status Changed**
```json
{"type": "block_status_changed", "block_id": "uuid", "status": "streaming|complete|error"}
```

**Error**
```json
{"type": "error", "message": "description"}
```

### Data Model

- **Journal**: A conversation container with title and timestamps
- **Block**: A message within a journal (user or assistant type) with content and status

Block statuses: `pending`, `streaming`, `complete`, `error`

## Development Setup

```bash
# Install dependencies and run
cargo run

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Run linter
cargo clippy
```

### Database Migrations

Migrations run automatically on startup. Schema is defined in `migrations/20260110_init.sql`.

### Project Structure

```
src/
├── main.rs       # Server setup and routing
├── error.rs      # Error types
├── models.rs     # Data models (Journal, Block)
├── opencode.rs   # OpenCode backend client
├── store.rs      # SQLite database operations
└── websocket.rs  # WebSocket handler and protocol
```

## License

MIT
