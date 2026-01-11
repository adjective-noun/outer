# Outer

A collaborative, multi-surface AI conversation platform where humans and agents are peers.

Outer wraps an OpenCode server to provide an infinite coding journal experience. Each prompt-response cycle is a "block" that can be forked and re-run. Multiple participants (human or AI) can attach to the same session with real-time CRDT-based sync.

## Key Features

- **Infinite Journal**: Conversations persist as blocks in a scrollable timeline
- **Fork & Re-run**: Branch from any point, re-execute prompts with different contexts
- **Real-time Collaboration**: Multiple users see changes instantly via CRDT sync
- **Symmetric Delegation**: Any participant can delegate work to any other (human or agent)
- **Approval Workflows**: Request and grant approvals in any direction
- **Multi-Surface**: Web UI, CLI with TUI, and programmatic WebSocket API

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Outer Server (Rust)                        │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Journal    │  │  Delegation  │  │    CRDT Sync         │  │
│  │   Store      │  │   Manager    │  │    (Yrs)             │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│           │               │                    │               │
│           └───────────────┼────────────────────┘               │
│                           ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                 OpenCode Bridge                          │  │
│  │   HTTP client to OpenCode, SSE multiplexing              │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────┬────────────────────────────────────┘
                            ▼
              ┌─────────────────────────┐
              │   OpenCode Server       │
              │   (AI Backend)          │
              └─────────────────────────┘
```

## Quick Start

```bash
# 1. Build everything
cargo build --release

# 2. Start OpenCode backend (required)
# See OpenCode documentation for setup

# 3. Run the Outer server (Terminal 1)
OPENCODE_URL=http://localhost:8080 ./target/release/outer
# Wait for "Server listening on 0.0.0.0:3000"

# 4a. Connect via CLI
./target/release/outer-cli connect ws://localhost:3000/ws

# 4b. OR use the Web UI (Terminal 2)
cd web && npm install && npm run dev
# Open http://localhost:5173 in your browser
```

**Important:** The Web UI requires the Outer server to be running. Start the server first, then the web dev server.

Or use the dev script to start both at once:
```bash
./scripts/dev.sh
```

### Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "Server Not Connected" overlay | Server not running | Run `cargo run` first |
| Buttons disabled | WebSocket not connected | Check server is on port 3000 |
| `ECONNREFUSED` in console | Server not running | Start server before web UI |

See [GETTING_STARTED.md](GETTING_STARTED.md) for a complete walkthrough.

## Project Structure

```
outer/
├── src/                    # Server core
│   ├── main.rs            # Entry point
│   ├── websocket.rs       # WebSocket protocol handler
│   ├── store.rs           # SQLite persistence
│   ├── opencode.rs        # OpenCode backend client
│   ├── models.rs          # Data models
│   ├── crdt/              # Real-time sync (Yrs)
│   │   ├── journal_doc.rs # CRDT document wrapper
│   │   ├── participant.rs # Presence tracking
│   │   └── room.rs        # Journal room manager
│   └── delegation/        # Work delegation system
│       ├── capability.rs  # Participant capabilities
│       ├── participant.rs # Registered participants
│       ├── work_item.rs   # Work items & approvals
│       └── manager.rs     # Delegation orchestration
├── cli/                   # CLI client with TUI
│   └── src/
│       ├── main.rs        # CLI entry point
│       ├── client.rs      # WebSocket client
│       ├── tui.rs         # Terminal UI (ratatui)
│       └── messages.rs    # Message types
├── web/                   # SvelteKit web interface
│   └── src/
│       ├── lib/           # Stores, types, WebSocket
│       └── routes/        # Pages and components
├── migrations/            # SQLite schema
├── tests/                 # Integration tests
└── docs/
    └── plans/             # Design documentation
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:outer.db` | SQLite connection string |
| `OPENCODE_URL` | `http://localhost:8080` | OpenCode backend URL |
| `RUST_LOG` | `outer=debug` | Logging level |
| `PORT` | `3000` | Server port |

## Surfaces

### Web UI

Rich visual interface with:
- Infinite scroll journal view
- Block cards with fork/rerun buttons
- Real-time presence indicators
- Approval panel for delegation workflows
- Responsive mobile design

```bash
cd web
npm install
npm run dev
```

### CLI

Terminal interface with ratatui TUI:
- Streaming response display
- Journal navigation
- Fork and rerun commands
- Agent/headless mode for automation

```bash
# Interactive mode
outer-cli connect ws://localhost:3000/ws

# Agent mode (no TUI)
outer-cli agent --journal <uuid>
```

### WebSocket API

Programmatic access for building integrations:

```javascript
const ws = new WebSocket('ws://localhost:3000/ws');

ws.send(JSON.stringify({
  type: 'submit',
  journal_id: 'uuid',
  content: 'Hello, AI!'
}));

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'block_content_delta') {
    process.stdout.write(msg.delta);
  }
};
```

## Participant Model

Outer treats humans and agents as peers with capability-based permissions:

```rust
enum Capability {
    Read,      // View journal content
    Submit,    // Send prompts
    Fork,      // Create branches
    Delegate,  // Assign work to others
    Approve,   // Approve/reject requests
    Admin,     // Manage participants
}
```

### Delegation Workflows

Any participant can delegate to any other:

```json
// Human delegates to agent
{"type": "delegate", "block_id": "...", "to": "agent-123", "note": "Implement auth"}

// Agent requests approval from human
{"type": "request_approval", "block_id": "...", "from": "user-456"}

// Human approves
{"type": "approve", "block_id": "..."}
```

## Development

```bash
# Run tests
cargo test

# Check coverage
cargo tarpaulin --out Stdout --ignore-tests

# Format code
cargo fmt

# Lint
cargo clippy
```

### Code Standards

- **80%+ test coverage** required for new code
- Tests must run against real OpenCode (not divergent mocks)
- Mock servers are acceptable for unit tests but must be validated against real backend

## License

MIT
