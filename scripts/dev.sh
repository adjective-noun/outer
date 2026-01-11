#!/bin/bash
# Development startup script for Outer
# Runs both the Rust server and the Vite dev server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting Outer development environment...${NC}"

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo not found. Please install Rust.${NC}"
    exit 1
fi

# Check if npm is available
if ! command -v npm &> /dev/null; then
    echo -e "${RED}Error: npm not found. Please install Node.js.${NC}"
    exit 1
fi

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

cd "$PROJECT_ROOT"

# Build if needed
if [ ! -f target/debug/outer ]; then
    echo -e "${YELLOW}Building Outer server...${NC}"
    cargo build
fi

# Install web dependencies if needed
if [ ! -d web/node_modules ]; then
    echo -e "${YELLOW}Installing web dependencies...${NC}"
    (cd web && npm install)
fi

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Shutting down...${NC}"
    kill $SERVER_PID 2>/dev/null || true
    kill $WEB_PID 2>/dev/null || true
    exit 0
}

trap cleanup SIGINT SIGTERM

# Start the Rust server in background
echo -e "${GREEN}Starting Outer server on port 3000...${NC}"
cargo run &
SERVER_PID=$!

# Wait for server to start
echo -e "${YELLOW}Waiting for server to start...${NC}"
sleep 2

# Check if server started
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo -e "${RED}Server failed to start${NC}"
    exit 1
fi

# Start the web dev server
echo -e "${GREEN}Starting web dev server on port 5173...${NC}"
(cd web && npm run dev) &
WEB_PID=$!

echo -e ""
echo -e "${GREEN}==============================================${NC}"
echo -e "${GREEN}Outer development environment running!${NC}"
echo -e "${GREEN}==============================================${NC}"
echo -e ""
echo -e "  Server:  http://localhost:3000"
echo -e "  Web UI:  http://localhost:5173"
echo -e ""
echo -e "Press Ctrl+C to stop both servers"
echo -e ""

# Wait for either process to exit
wait
