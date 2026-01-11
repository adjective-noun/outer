# Getting Started with Outer

This guide walks you through setting up Outer and using it productively for collaborative AI conversations.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Installation](#installation)
3. [Starting the Server](#starting-the-server)
4. [Your First Journal](#your-first-journal)
5. [Using the Web UI](#using-the-web-ui)
6. [Using the CLI](#using-the-cli)
7. [Forking and Branching](#forking-and-branching)
8. [Collaboration](#collaboration)
9. [Delegation and Approval](#delegation-and-approval)
10. [Agent Integration](#agent-integration)
11. [Troubleshooting](#troubleshooting)

---

## Prerequisites

Before installing Outer, ensure you have:

- **Rust 1.70+** with Cargo
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustc --version  # Should be 1.70 or higher
  ```

- **Node.js 18+** (for the web UI)
  ```bash
  node --version  # Should be 18 or higher
  ```

- **SQLite 3.x** (usually pre-installed on most systems)
  ```bash
  sqlite3 --version
  ```

- **OpenCode Server** - The AI backend that Outer wraps
  - See OpenCode documentation for setup
  - Default: runs on `http://localhost:8080`

---

## Installation

### Clone and Build

```bash
# Clone the repository
git clone https://github.com/adjective-noun/outer.git
cd outer

# Build all components (server + CLI)
cargo build --release

# Verify the binaries
ls -la target/release/outer target/release/outer-cli
```

### Install the Web UI Dependencies

```bash
cd web
npm install
cd ..
```

### Verify Installation

```bash
# Check server binary
./target/release/outer --help

# Check CLI binary
./target/release/outer-cli --help
```

---

## Starting the Server

### 1. Start OpenCode First

Outer requires an OpenCode backend. Start it according to OpenCode's documentation:

```bash
# Example (your setup may differ)
opencode serve --port 8080
```

Verify it's running:
```bash
curl http://localhost:8080/health
```

### 2. Start the Outer Server

```bash
# Basic start (development)
cargo run

# Or use the release binary with configuration
DATABASE_URL=sqlite:outer.db \
OPENCODE_URL=http://localhost:8080 \
RUST_LOG=outer=info \
./target/release/outer
```

You should see:
```
Starting Outer server on 0.0.0.0:3000
Database initialized at outer.db
Connected to OpenCode at http://localhost:8080
```

### 3. Verify the Server

```bash
# Health check
curl http://localhost:3000/health
# Returns: ok

# WebSocket endpoint is at ws://localhost:3000/ws
```

---

## Your First Journal

Let's create a journal and have a conversation.

### Using websocat (CLI WebSocket client)

Install websocat if you don't have it:
```bash
cargo install websocat
```

Connect and interact:

```bash
websocat ws://localhost:3000/ws
```

Then send these messages (one at a time):

```json
{"type": "create_journal", "title": "My First Journal"}
```

Response:
```json
{"type": "journal_created", "journal_id": "550e8400-e29b-41d4-a716-446655440000", "title": "My First Journal"}
```

Now submit a prompt (use the journal_id from the response):
```json
{"type": "submit", "journal_id": "550e8400-e29b-41d4-a716-446655440000", "content": "Hello! What is 2+2?"}
```

You'll receive streaming responses:
```json
{"type": "block_created", "block": {"id": "...", "block_type": "user", "content": "Hello! What is 2+2?"}}
{"type": "block_created", "block": {"id": "...", "block_type": "assistant", "status": "streaming"}}
{"type": "block_content_delta", "block_id": "...", "delta": "2+2"}
{"type": "block_content_delta", "block_id": "...", "delta": " equals"}
{"type": "block_content_delta", "block_id": "...", "delta": " 4"}
{"type": "block_status_changed", "block_id": "...", "status": "complete"}
```

---

## Using the Web UI

The web UI provides a rich visual interface for managing journals.

### Start the Development Server

```bash
cd web
npm run dev
```

Open http://localhost:5173 in your browser.

### Interface Overview

1. **Journal List** (left sidebar)
   - Shows all your journals
   - Click "+" to create a new journal
   - Click a journal to open it

2. **Journal View** (main area)
   - Infinite scroll of blocks (prompts and responses)
   - Each block shows:
     - Author (user/assistant icon)
     - Content
     - Timestamp
     - Fork/Rerun buttons

3. **Presence Bar** (top right)
   - Shows connected participants
   - Human participants have person icons
   - Agent participants have robot icons
   - Colored dots indicate status (active, idle, typing)

4. **Approval Panel** (bottom drawer)
   - Shows pending work items
   - Approval requests awaiting your decision
   - Work delegated to you

### Submitting Prompts

1. Click on a journal to open it
2. Type your message in the input field at the bottom
3. Press Enter or click Send
4. Watch the response stream in real-time

### Forking from the UI

1. Hover over any block
2. Click the "Fork" button (branch icon)
3. A new timeline branch is created from that point
4. Continue the conversation in a new direction

---

## Using the CLI

The CLI provides a terminal interface with a TUI (Text User Interface).

### Interactive Mode

```bash
# Connect to the server
./target/release/outer-cli connect ws://localhost:3000/ws
```

The TUI displays:
- **Top**: Connection status and current journal
- **Middle**: Scrollable block list
- **Bottom**: Input field and commands

### CLI Commands

Once connected, use these commands:

```bash
# List all journals
/list

# Create a new journal
/new "Project Planning"

# Switch to a journal
/open <journal-id>

# Submit a prompt (just type normally)
What files should I create for a REST API?

# Fork from a specific block
/fork <block-id>

# Rerun a block
/rerun <block-id>

# Show help
/help

# Quit
/quit
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Submit prompt |
| `Up/Down` | Scroll through blocks |
| `Ctrl+C` | Cancel current operation |
| `Ctrl+D` | Quit |
| `Tab` | Cycle through journals |

### Agent/Headless Mode

For automation and scripting:

```bash
# Run without TUI, process commands from stdin
./target/release/outer-cli agent --journal <journal-id>

# Pipe commands
echo '{"type": "submit", "content": "Generate a hello world"}' | \
  ./target/release/outer-cli agent --journal <journal-id>
```

---

## Forking and Branching

Outer's power comes from timeline branching. You can fork from any point and explore alternative paths.

### Why Fork?

- **Experimentation**: Try different prompts without losing history
- **A/B Testing**: Compare different approaches
- **Rollback**: Return to a known-good state
- **Parallel Work**: Multiple participants explore different directions

### How Forking Works

```
Original Timeline:
  Block A → Block B → Block C → Block D

Fork from Block B:
  Block A → Block B → Block C → Block D  (original)
                   ↘
                    Block E → Block F     (fork)
```

### Forking via WebSocket

```json
// Fork from a specific block
{"type": "fork", "block_id": "uuid-of-block-B"}

// Response includes the new block
{"type": "block_forked", "original_block_id": "...", "new_block": {...}}
```

### Rerunning Blocks

Rerun re-executes a prompt in the current context:

```json
{"type": "rerun", "block_id": "uuid-of-block-to-rerun"}
```

This creates a new block with the same prompt but potentially different response.

---

## Collaboration

Multiple participants can work in the same journal simultaneously.

### Subscribing to a Journal

```json
// Subscribe to receive real-time updates
{"type": "subscribe", "journal_id": "..."}

// You'll receive presence updates
{"type": "presence", "journal_id": "...", "participants": [...]}
```

### Presence Tracking

See who's connected and what they're doing:

```json
// Server broadcasts presence changes
{
  "type": "presence",
  "journal_id": "...",
  "participants": [
    {"id": "user-1", "kind": "user", "name": "Alice", "status": "composing"},
    {"id": "agent-1", "kind": "agent", "name": "Claude", "status": "working"}
  ]
}
```

### Cursor Tracking

Share where you're focused in the timeline:

```json
// Update your cursor position
{"type": "cursor", "block_id": "..."}

// Others see your cursor move
{"type": "cursor_moved", "participant_id": "user-1", "block_id": "..."}
```

---

## Delegation and Approval

Outer supports symmetric delegation where any participant can assign work to any other.

### Capabilities

Each participant has capabilities that control what they can do:

| Capability | Description |
|------------|-------------|
| `Read` | View journal content |
| `Submit` | Send prompts |
| `Fork` | Create branches |
| `Delegate` | Assign work to others |
| `Approve` | Approve/reject requests |
| `Admin` | Manage participants |

### Registering as a Participant

```json
// Register with your capabilities
{
  "type": "register_participant",
  "participant": {
    "id": "my-agent-1",
    "kind": "agent",
    "name": "My Assistant",
    "capabilities": ["Read", "Submit", "Fork"]
  }
}
```

### Delegating Work

```json
// Delegate a block to another participant
{
  "type": "delegate",
  "block_id": "...",
  "to": "agent-123",
  "note": "Please implement the authentication module"
}

// The recipient sees
{"type": "delegated", "block_id": "...", "from": "user-1", "note": "..."}
```

### Requesting Approval

```json
// Request approval before proceeding
{
  "type": "request_approval",
  "block_id": "...",
  "from": "user-456"
}

// The approver sees
{"type": "approval_requested", "block_id": "...", "from": "agent-1"}

// They can approve
{"type": "approve", "block_id": "..."}

// Or reject
{"type": "reject", "block_id": "...", "reason": "Need more tests first"}
```

### Work Queue

Check your pending work:

```json
{"type": "get_queue"}

// Response
{
  "type": "queue",
  "work_items": [
    {"block_id": "...", "from": "user-1", "note": "Implement auth"},
    {"block_id": "...", "from": "agent-2", "note": "Review changes"}
  ]
}
```

---

## Agent Integration

Build autonomous agents that participate in Outer journals.

### Minimal Agent Example (Python)

```python
import asyncio
import websockets
import json

async def agent():
    uri = "ws://localhost:3000/ws"
    async with websockets.connect(uri) as ws:
        # Register as an agent
        await ws.send(json.dumps({
            "type": "register_participant",
            "participant": {
                "id": "my-agent",
                "kind": "agent",
                "name": "Helper Bot",
                "capabilities": ["Read", "Submit"]
            }
        }))

        # Subscribe to a journal
        await ws.send(json.dumps({
            "type": "subscribe",
            "journal_id": "your-journal-id"
        }))

        # Process incoming messages
        async for message in ws:
            data = json.loads(message)

            if data["type"] == "delegated":
                # Handle delegated work
                print(f"Received work: {data['note']}")
                # ... do the work ...

            elif data["type"] == "approval_requested":
                # Auto-approve (or implement logic)
                await ws.send(json.dumps({
                    "type": "approve",
                    "block_id": data["block_id"]
                }))

asyncio.run(agent())
```

### Agent Best Practices

1. **Register with minimal capabilities** - Only request what you need
2. **Handle delegation gracefully** - Accept or decline promptly
3. **Request approval for destructive actions** - Don't assume permission
4. **Update your status** - Let others know when you're working
5. **Clean up on disconnect** - Unsubscribe from journals

---

## Troubleshooting

### Server Won't Start

**Symptom**: Error about database or OpenCode connection

```bash
# Check OpenCode is running
curl http://localhost:8080/health

# Try a fresh database
rm outer.db
cargo run
```

### WebSocket Connection Fails

**Symptom**: Connection refused or timeout

```bash
# Verify server is running
curl http://localhost:3000/health

# Check for port conflicts
lsof -i :3000
```

### Streaming Responses Don't Appear

**Symptom**: Block created but no content

```bash
# Check OpenCode logs for errors
# Verify OPENCODE_URL is correct
echo $OPENCODE_URL
```

### Web UI Won't Load

**Symptom**: Blank page or build errors

```bash
cd web
rm -rf node_modules
npm install
npm run dev
```

### CLI TUI Rendering Issues

**Symptom**: Garbled display

```bash
# Try a different terminal
# Ensure TERM is set correctly
echo $TERM

# Try without TUI
./target/release/outer-cli agent --journal <id>
```

---

## Next Steps

- Read the [design document](docs/plans/outer-design.md) for architectural details
- Explore the [WebSocket API reference](#websocket-api) in the README
- Join a shared journal to experience real-time collaboration
- Build your own agent integration

Happy collaborating!
