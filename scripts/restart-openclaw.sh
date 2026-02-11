#!/bin/bash
# Restart openclaw gateway and update cam binary + skills

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OPENCLAW_SKILLS_DIR="$HOME/.openclaw/skills"

# Build and update cam
echo "Building cam..."
cd "$PROJECT_DIR"
cargo build --release

echo "Updating cam binary..."
cp target/release/cam plugins/cam/bin/cam
cp target/release/cam ~/.local/bin/cam 2>/dev/null || true

# Update skills
echo "Updating skills..."
mkdir -p "$OPENCLAW_SKILLS_DIR"

# Copy cam-related skills to openclaw
for skill in cam agent-teams cam-notify; do
    if [ -d "$PROJECT_DIR/skills/$skill" ]; then
        rm -rf "$OPENCLAW_SKILLS_DIR/$skill"
        cp -r "$PROJECT_DIR/skills/$skill" "$OPENCLAW_SKILLS_DIR/$skill"
        echo "  - $skill"
    fi
done

# Kill existing watcher if running
if [ -f ~/.claude-monitor/watcher.pid ]; then
    OLD_PID=$(cat ~/.claude-monitor/watcher.pid)
    if kill -0 "$OLD_PID" 2>/dev/null; then
        echo "Stopping old watcher (PID: $OLD_PID)..."
        kill "$OLD_PID" 2>/dev/null || true
        sleep 1
    fi
fi

# Restart gateway
echo "Restarting openclaw gateway..."
openclaw gateway restart

echo "✅ cam updated and gateway restarted successfully"
