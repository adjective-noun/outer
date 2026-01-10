# Outer.sh Design Plan

## Overview

**Outer.sh** is a multi-surface, multi-user coding environment that wraps an OpenCode server. The UX is an infinite coding journal where each prompt→agent-loop chain is a "block" that can be forked and re-run. Multiple participants can attach to the same session with real-time sync.

### Collaboration Modes

Outer.sh supports peer-to-peer collaboration where **humans and agents are equals**:

1. **Participant ↔ Participant** - Any participant (human or agent) can:
   - Submit work to the journal
   - Delegate work to another participant
   - Accept work delegated to them
   - Request approval from another participant
   - Approve or reject requests from others

2. **No assumed hierarchy** - Management flows in any direction:
   - Agent delegates to human ("Please review this PR")
   - Human delegates to agent ("Implement auth module")
   - Agent delegates to agent ("You handle tests, I'll do impl")
   - Human delegates to human (pair programming)

3. **Role-based, not type-based** - Capabilities are per-participant, not per-species:
   - A senior agent might approve a junior human's code
   - A human might be read-only observer on an agent's work
   - Trust is earned through the capability ledger, not assumed

### Participant Model

```rust
// Humans and agents are peers - differentiated only for UX optimization
enum ParticipantKind {
    Human { user_id: String },
    Agent { agent_id: String },
}

struct Participant {
    id: String,
    kind: ParticipantKind,
    name: String,           // Display name
    capabilities: Vec<Capability>,
    // Who spawned this participant (for accountability chain)
    spawned_by: Option<String>,
}

enum Capability {
    Read,           // Can see journal
    Submit,         // Can send prompts
    Fork,           // Can create branches
    Delegate,       // Can assign work to others
    Approve,        // Can approve/reject requests
    Admin,          // Can manage participants
}
```

### UX Principle: S-Tier for Both

The interface must be excellent for both humans AND agents:

**For Humans:**
- Rich visual UI (web), responsive TUI (CLI)
- Real-time presence, cursor tracking
- Intuitive fork/branch visualization
- Clear attribution (who did what)

**For Agents:**
- Clean programmatic API (WebSocket + JSON)
- Structured events for state changes
- Efficient polling/streaming for updates
- Machine-readable block structure
- Clear work queue / assignment protocol

**Shared:**
- Same wire protocol for both
- Same capabilities model
- Same delegation/approval mechanics
- Symmetric handoff in both directions

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Outer.sh Server (Rust)                     │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Journal    │  │   Session    │  │    CRDT Sync         │  │
│  │   Store      │  │   Broker     │  │    (Yrs/Automerge)   │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│           │               │                    │                │
│           └───────────────┼────────────────────┘                │
│                           ▼                                     │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                 OpenCode Bridge                          │   │
│  │   HTTP client to OpenCode, SSE multiplexing              │   │
│  └─────────────────────────────────────────────────────────┘   │
└───────────────────────────┬─────────────────────────────────────┘
                            ▼
              ┌─────────────────────────┐
              │   OpenCode Server       │
              │   (localhost:4096)      │
              └─────────────────────────┘
```

**Surfaces:**
- Web UI (SvelteKit or vanilla HTML/WASM)
- CLI client (Rust, talks WebSocket to server)

## Data Model

```rust
// Journal = collection of blocks forming an infinite scroll
struct Journal {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    root_block_id: Option<Uuid>,  // First block
}

// Block = one prompt→response cycle
struct Block {
    id: Uuid,
    journal_id: Uuid,
    parent_id: Option<Uuid>,           // Previous block in timeline
    forked_from_id: Option<Uuid>,      // Block this was forked from

    // OpenCode mapping
    opencode_session_id: String,
    opencode_message_id: Option<String>,

    // Content
    prompt: String,
    response_parts: Vec<ResponsePart>,
    status: BlockStatus,  // Pending, Running, Complete, Error, Cancelled

    // Metadata
    model: Option<String>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

enum ResponsePart {
    Text { content: String },
    ToolCall { id: String, name: String, input: Value },
    ToolResult { call_id: String, output: Value },
    Error { message: String },
}

// Participant = who's connected (human or agent)
struct Participant {
    id: String,
    kind: ParticipantKind,
    journal_id: Uuid,
    cursor_block_id: Option<Uuid>,
    color: String,              // For UI cursor colors
    status: ParticipantStatus,  // What they're doing
    capabilities: Vec<Capability>,
}

enum ParticipantStatus {
    Idle,
    Composing,        // Drafting a prompt
    Working,          // Actively executing
    WaitingApproval,  // Blocked on another participant's approval
    Observing,        // Read-only watcher
}

// Block attribution
struct BlockAuthor {
    participant_id: String,
    kind: ParticipantKind,
    delegated_by: Option<String>,  // If agent, who spawned it
}
```

## Wire Protocol (WebSocket)

Same protocol for humans and agents - no special cases.

```typescript
// Client → Server
type ClientMessage =
  | { type: "auth", token: string, participant_id: string }
  | { type: "subscribe", journal_id: string }
  | { type: "unsubscribe", journal_id: string }
  | { type: "submit", journal_id: string, prompt: string, after_block_id?: string }
  | { type: "fork", block_id: string }
  | { type: "rerun", block_id: string }
  | { type: "cancel", block_id: string }
  | { type: "cursor", block_id: string }
  // Delegation (works any direction)
  | { type: "delegate", block_id: string, to: string, note?: string }
  | { type: "accept", block_id: string }     // Accept delegated work
  | { type: "decline", block_id: string, reason?: string }
  // Approval (works any direction)
  | { type: "request_approval", block_id: string, from: string }
  | { type: "approve", block_id: string }
  | { type: "reject", block_id: string, reason?: string }
  // Queue management
  | { type: "get_queue" }                    // Request my work queue
  | { type: "claim", block_id: string }      // Claim work from queue
  // Presence
  | { type: "status", status: ParticipantStatus }  // Update my status
  | { type: "watch", participant_id: string }      // Watch another participant

// Server → Client
type ServerMessage =
  | { type: "sync", journal_id: string, changes: Uint8Array }
  | { type: "stream", block_id: string, part: ResponsePart }
  | { type: "presence", journal_id: string, participants: Participant[] }
  | { type: "error", message: string }
  // Work notifications (same for all participants)
  | { type: "delegated", block_id: string, from: string, note?: string }
  | { type: "approval_requested", block_id: string, from: string }
  | { type: "approved", block_id: string, by: string }
  | { type: "rejected", block_id: string, by: string, reason?: string }
  | { type: "cancelled", block_id: string, by: string }
  // Queue
  | { type: "queue", blocks: Block[] }
  // Status updates
  | { type: "participant_status", participant_id: string, status: ParticipantStatus }
```

## Interaction Patterns (Symmetric)

All patterns work identically regardless of whether participants are humans or agents.

### Pattern 1: Delegation
```
Participant A submits: "Implement the auth module" → delegates to B
  → Block created, assigned to B
  → B picks up, works autonomously
  → B submits sub-blocks with implementation
  → A sees progress in real-time
  → A can intervene/redirect at any point

Examples:
  - Human → Agent: "Implement auth"
  - Agent → Human: "Please review this PR"
  - Agent → Agent: "You handle tests"
  - Human → Human: "Can you take this?"
```

### Pattern 2: Approval Request
```
Participant A submits: "Ready to delete old schema" → requests approval from B
  → Block marked "approval_needed", assigned to B
  → B receives notification
  → B approves or rejects with feedback
  → A continues or adjusts based on response

Examples:
  - Agent → Human: "Approve this migration?"
  - Human → Agent: "Does this look right to you?"
  - Agent → Agent: "Sign off on my implementation?"
```

### Pattern 3: Pipeline (Multi-Participant)
```
Journal with three participants (any mix of humans/agents):
  1. Planner: Creates implementation plan blocks
  2. Implementer: Watches planner, implements each step
  3. Reviewer: Watches implementer, adds review comments

Each participant has their own cursor/focus
All activity visible to all subscribers
```

### Pattern 4: Takeover / Interrupt
```
Participant A is working on block
Participant B sends: { type: "cancel", block_id: "..." }
B sends new prompt, taking over
A receives "cancelled" event, yields control

Works in any direction - agents can interrupt humans too
(e.g., "I found a critical bug, taking over to fix")
```

### Pattern 5: Work Queue
```
Participant checks their queue:
  → GET /api/me/queue (or subscribe to queue events)
  → Returns blocks assigned to them, pending action
  → Participant claims and works each item
  → Marks complete when done

Same queue model for humans and agents.
Agents poll/subscribe, humans see UI notification.
```

## Key Components

### 1. Journal Store (`src/store/`)
- SQLite for persistence (journals, blocks, users)
- CRDT document per journal (Yrs - Rust port of Yjs)
- Journal state = CRDT doc, syncs to all subscribers

### 2. Session Broker (`src/broker/`)
- Maps Journal → OpenCode sessions
- On fork: creates new OpenCode child session
- Multiplexes SSE events from OpenCode to N WebSocket clients
- Handles reconnection, session cleanup

### 3. OpenCode Bridge (`src/bridge/`)
- HTTP client using `reqwest`
- SSE client using `eventsource-client` or `reqwest-eventsource`
- Methods:
  - `create_session(project: &str) -> Session`
  - `fork_session(session_id: &str, at_message: &str) -> Session`
  - `send_message(session_id: &str, prompt: &str) -> Stream<ResponsePart>`
  - `subscribe_events(session_id: &str) -> Stream<Event>`

### 4. WebSocket Server (`src/ws/`)
- `tokio` + `axum` for HTTP/WS
- Each connection → actor that handles ClientMessages
- Broadcasts CRDT changes to room (journal subscribers)
- Relays OpenCode SSE → block stream events

### 5. CRDT Engine (`src/crdt/`)
- Yrs (Rust Yjs) for journal documents
- Each journal = YDoc with:
  - `blocks`: YMap<block_id, Block>
  - `timeline`: YArray<block_id>  // ordered list
  - `forks`: YMap<block_id, [child_block_ids]>
- Merge conflict-free across clients

## Directory Structure

```
outer/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point, CLI args
│   ├── config.rs            # Configuration
│   ├── lib.rs               # Library root
│   │
│   ├── api/                 # HTTP/WS API layer
│   │   ├── mod.rs
│   │   ├── routes.rs        # axum routes
│   │   ├── ws.rs            # WebSocket handler
│   │   └── auth.rs          # Simple auth (tokens)
│   │
│   ├── store/               # Persistence
│   │   ├── mod.rs
│   │   ├── db.rs            # SQLite via sqlx
│   │   ├── journal.rs       # Journal CRUD
│   │   └── block.rs         # Block CRUD
│   │
│   ├── broker/              # Session management
│   │   ├── mod.rs
│   │   ├── session.rs       # OpenCode session wrapper
│   │   └── room.rs          # Journal room (subscribers)
│   │
│   ├── bridge/              # OpenCode client
│   │   ├── mod.rs
│   │   ├── client.rs        # HTTP client
│   │   ├── sse.rs           # SSE stream handler
│   │   └── types.rs         # OpenCode API types
│   │
│   └── crdt/                # Real-time sync
│       ├── mod.rs
│       └── journal_doc.rs   # Yrs document structure
│
├── cli/                     # CLI client (separate crate)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
│
└── web/                     # Web UI (optional, could be separate repo)
    ├── package.json
    └── src/
```

## Implementation Phases

### Phase 1: Core Server + Single User
1. Set up Rust project with axum, tokio, sqlx
2. Implement OpenCode Bridge (HTTP client, SSE streaming)
3. Basic Journal/Block store (SQLite)
4. WebSocket server with submit/stream
5. Single-user flow: submit prompt → stream response → save block

**Files:** `main.rs`, `config.rs`, `api/*`, `store/*`, `bridge/*`

### Phase 2: Fork & Re-run
1. Implement block forking (create new OpenCode session from parent)
2. Re-run capability (same prompt, new execution)
3. Timeline branching in data model
4. WebSocket commands: fork, rerun, cancel

**Files:** `broker/*`, extend `store/block.rs`

### Phase 3: Multi-User Real-Time Sync
1. Integrate Yrs for CRDT
2. Journal room concept (subscribers to a journal)
3. Broadcast CRDT changes on block updates
4. Presence tracking (cursor positions, user list)

**Files:** `crdt/*`, extend `broker/room.rs`, `api/ws.rs`

### Phase 4: Delegation & Approval System
1. Participant registration (both human and agent)
2. Capability checks on all operations
3. Symmetric delegation protocol (any→any)
4. Work queue per participant
5. Approval request/response flow
6. Status broadcasting for all participants

**Files:** extend `api/auth.rs`, new `src/delegation/` module

### Phase 5: CLI Client
1. WebSocket client in Rust
2. TUI using ratatui
3. Commands: connect, submit, fork, list journals
4. Agent mode: `outer agent --journal <id>` (headless, for spawning agents)

**Files:** `cli/*`

### Phase 6: Web UI
1. SvelteKit or vanilla HTML + y-websocket
2. Infinite scroll journal view
3. Block cards with prompt/response + author attribution
4. Fork/rerun buttons
5. Participant presence indicators (humans vs agents)
6. Approval UI for human-in-the-loop gates

## Verification

1. **Unit tests**: Bridge mocking, CRDT merge scenarios
2. **Integration tests**:
   - Start OpenCode server
   - Start Outer server
   - Submit prompt via WebSocket, verify block creation
   - Fork block, verify new session
3. **Multi-participant sync tests**:
   - Two clients subscribe to same journal
   - One submits, other sees real-time update
   - CRDT convergence after concurrent edits
4. **Delegation tests (symmetric)**:
   - Human delegates to agent, agent completes
   - Agent delegates to human, human completes
   - Agent delegates to agent (pipeline)
   - Approval flow in all directions
   - Decline/reject handling
5. **Work queue tests**:
   - Participant receives delegated work in queue
   - Claim, complete, and clear from queue
   - Multiple pending items
6. **CLI test**:
   - `outer connect ws://localhost:8080`
   - `outer submit "hello world"`
   - `outer --headless` (agent/automation mode)
   - Verify streaming output

## Open Questions (for implementation)

1. **Persistence format**: SQLite + Yrs snapshots, or full Yrs persistence?
2. **Auth model**: Simple tokens for MVP, or OAuth later?
3. **OpenCode project**: One project per journal, or shared project?
4. **Block storage**: Store full response or just OpenCode message ID + fetch on demand?

## Dependencies (Cargo.toml)

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.7", features = ["ws"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
reqwest-eventsource = "0.6"
yrs = "0.21"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```
