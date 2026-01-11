#!/bin/bash
# Development startup script for Outer
# Spawns tmux panes for the Rust server (with auto-reload) and Vite dev server

set -e

PROJECT_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )/.." && pwd )"

# Check for cargo-watch
if ! command -v cargo-watch &> /dev/null; then
    echo "Installing cargo-watch..."
    cargo install cargo-watch
fi

if [ -n "$TMUX" ]; then
    # Already in tmux - split current pane vertically
    tmux split-window -v -c "$PROJECT_ROOT/web" "npm run dev"
    tmux select-pane -U
    exec cargo watch -x run --workdir "$PROJECT_ROOT"
else
    # Not in tmux - create new session
    SESSION="outer-dev"
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    tmux new-session -d -s "$SESSION" -c "$PROJECT_ROOT" -n main "cargo watch -x run"
    tmux split-window -v -t "$SESSION" -c "$PROJECT_ROOT/web" "npm run dev"
    tmux select-pane -t "$SESSION" -U
    tmux attach -t "$SESSION"
fi
