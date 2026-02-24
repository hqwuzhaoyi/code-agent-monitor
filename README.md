# Code Agent Monitor (CAM)

[English](README.md) | [中文](README.zh-CN.md)

Monitor and manage AI coding agent processes (Claude Code, OpenCode, Codex).

## Features

- **TUI Dashboard** - Terminal UI for monitoring agents with real-time status, filtering, and tmux integration
- **Process Monitoring** - Scan all running AI coding agents in the system
- **Multi-Agent Adapter** - Unified abstraction layer supporting Claude Code, Codex CLI, OpenCode with automatic detection
- **Session Management** - List and resume Claude Code historical sessions
- **Agent Lifecycle** - Start, stop, and send input to agents
- **Smart Notifications** - Route notifications based on urgency (HIGH/MEDIUM/LOW)
- **Terminal Snapshots** - Include recent terminal output in notifications for remote context
- **MCP Server** - Provide MCP protocol interface for other tools
- **OpenClaw Integration** - Manage agents via natural language
- **Agent Teams** - Multi-agent collaboration with remote management and quick replies
- **Risk Assessment** - Automatically evaluate permission request risk levels
- **Service Management** - Install watcher as launchd system service (macOS) with auto-start on boot

## Installation

### Prerequisites

- Rust 1.70+
- tmux
- Claude Code CLI (optional, for agent management)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/hqwuzhaoyi/code-agent-monitor.git
cd code-agent-monitor

# Build release binary
cargo build --release

# Binary location
./target/release/cam

# Optional: Install to PATH
cp target/release/cam /usr/local/bin/
```

### OpenClaw Plugin Installation

```bash
# Install as OpenClaw plugin
openclaw plugins install --link /path/to/code-agent-monitor/plugins/cam
openclaw gateway restart
```

## Usage

### Basic Commands

```bash
# List all agent processes
cam list

# List historical sessions
cam sessions

# Resume a session to tmux
cam resume <session_id>

# View session logs
cam logs <session_id> --limit 10

# Kill a process
cam kill <pid>

# Start MCP server
cam serve

# Start background watcher daemon
cam watch-daemon -i 3

# Manually trigger watcher detection and send notification
cam watch-trigger --agent-id <agent_id> [--force] [--no-dedup]

# Launch TUI dashboard
cam tui

# Install watcher as system service (macOS)
cam install                       # Install service
cam install --force               # Force reinstall
cam uninstall                     # Uninstall service

# Service management
cam service status                # View service status
cam service restart               # Restart service
cam service logs                  # View service logs
cam service logs -f               # Follow logs
```

### TUI Dashboard

The TUI provides a real-time dashboard for monitoring all running agents with lazygit-style filtering.

```bash
# Launch TUI
cam tui
```

Features:
- Real-time agent list with status indicators (Running/Idle/Error)
- Live terminal preview of selected agent's tmux session
- Lazygit-style instant filtering (type to filter as you type)
- Log viewer with level filtering (Error/Warn/Info/Debug)
- Notifications panel with urgency-based color coding (HIGH=Red, MEDIUM=Yellow, LOW=Gray)
- Local notification storage with automatic rolling cleanup (keeps last 100 entries)

Key bindings:
| Key | Action |
|-----|--------|
| `j/k` or `↑/↓` | Navigate agents |
| `/` | Enter filter mode (type to filter by ID or project) |
| `Enter` | Attach to selected agent's tmux session |
| `l` | Switch to logs view |
| `f` | Toggle log level filter (in logs view) |
| `Esc` | Clear filter / Return to dashboard |
| `q` | Quit |

### Notification Commands

```bash
# Send notification event
cam notify --event stop --agent-id cam-xxx

# Preview notification (dry-run)
echo '{"cwd": "/tmp"}' | cam notify --event stop --agent-id cam-xxx --dry-run
```

### Team Commands

```bash
# Create a team
cam team-create my-project --description "My project"

# Spawn an agent in team
cam team-spawn my-project developer --prompt "Analyze project structure"

# View team progress
cam team-progress my-project

# Shutdown team
cam team-shutdown my-project
```

### Quick Reply Commands

```bash
# View pending confirmations
cam pending-confirmations

# Reply to single pending confirmation
cam reply y [--target <agent_id>]

# Batch reply (Agent Swarm scenarios)
cam reply y --all                 # Approve all pending
cam reply y --agent "cam-*"       # Approve matching agents
cam reply y --risk low            # Approve all LOW risk requests

# Setup hooks for different CLI tools
cam setup claude                  # Configure Claude Code hooks
cam setup codex                   # Configure Codex CLI notify
cam setup --dry-run claude        # Preview changes without applying
```

### Multi-Agent CLI Support

CAM supports multiple AI coding CLI tools through a unified adapter layer:

| CLI Tool | Detection Strategy | Hook Events |
|----------|-------------------|-------------|
| Claude Code | HookOnly | session_start, stop, notification, PreToolUse, PostToolUse |
| Codex CLI | HookWithPolling | agent-turn-complete |
| OpenCode | HookOnly | session.created, session.idle, permission.asked, tool.execute.* |
| Others | PollingOnly | (terminal state detection via AI) |

```bash
# Setup hooks automatically
cam setup claude                  # Configure Claude Code hooks
cam setup codex                   # Configure Codex notify
cam setup opencode                # Configure OpenCode plugin

# Handle Codex CLI events
cam codex-notify '{"type":"agent-turn-complete","thread-id":"xxx"}'
```

## Configuration

### Webhook Configuration (Required for Notifications)

CAM sends notifications via Webhook to OpenClaw Gateway. Configure in `~/.config/code-agent-monitor/config.json`:

```json
{
  "webhook": {
    "gateway_url": "http://localhost:18789",
    "hook_token": "your-webhook-token",
    "timeout_secs": 30
  }
}
```

### Haiku API Configuration

CAM uses Claude Haiku 4.5 for terminal state detection and question extraction. API configuration is read in the following priority:

1. `~/.config/code-agent-monitor/config.json` (recommended)
2. Environment variables `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL`
3. `~/.anthropic/api_key`
4. `~/.openclaw/openclaw.json`

**Configuration example** (`~/.config/code-agent-monitor/config.json`):

```json
{
  "anthropic_api_key": "sk-xxx",
  "anthropic_base_url": "http://localhost:23000/"
}
```

### Claude Code Hooks Configuration

To enable automatic notifications when Claude Code is idle:

**Automatic configuration (recommended)**:

```bash
# Get CAM plugin path
CAM_BIN=$(openclaw plugins list --json | jq -r '.[] | select(.name == "cam") | .path')/bin/cam

# Add hooks to Claude Code config
cat ~/.claude/settings.json | jq --arg cam "$CAM_BIN" '.hooks = {
  "Notification": [{
    "matcher": "idle_prompt",
    "hooks": [{
      "type": "command",
      "command": ($cam + " notify --event idle_prompt --agent-id $SESSION_ID")
    }]
  }]
}' > ~/.claude/settings.json.tmp && mv ~/.claude/settings.json.tmp ~/.claude/settings.json
```

**Manual configuration**:

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Notification": [
      {
        "matcher": "idle_prompt",
        "hooks": [
          {
            "type": "command",
            "command": "<CAM_PLUGIN_PATH>/bin/cam notify --event idle_prompt --agent-id $SESSION_ID"
          }
        ]
      }
    ]
  }
}
```

## Debugging

### View Logs

```bash
# View hook logs
tail -f ~/.config/code-agent-monitor/hook.log

# View watcher logs
tail -f ~/.config/code-agent-monitor/watcher.log

# Check watcher status
cat ~/.config/code-agent-monitor/watcher.pid
```

### Dry-Run Testing

```bash
# Preview HIGH urgency notification
echo '{"cwd": "/workspace"}' | cam notify --event permission_request --agent-id cam-test --dry-run

# Preview MEDIUM urgency notification
echo '{"cwd": "/workspace"}' | cam notify --event stop --agent-id cam-test --dry-run
```

### Verify Channel Detection

```bash
# Check OpenClaw channel configuration
cat ~/.openclaw/openclaw.json | jq '.channels'

# Test channel detection
echo '{}' | cam notify --event stop --agent-id test --dry-run 2>&1 | grep "channel="
```

### Common Issues

| Issue | Solution |
|-------|----------|
| Notifications not sending | Check `~/.config/code-agent-monitor/hook.log` for records |
| Send failures | Check stderr output, may be network or API rate limiting |
| Wrong routing | Use `--dry-run` to verify urgency classification |
| Channel detection failed | Check `~/.openclaw/openclaw.json` configuration |
| New format not applied | Restart watcher daemon |

### Restart Watcher

After code changes, restart the watcher:

```bash
# If running as service (recommended)
cam service restart

# If running manually
kill $(cat ~/.config/code-agent-monitor/watcher.pid) 2>/dev/null
# Watcher will auto-start on next agent launch
```

## Architecture

CAM uses a layered architecture with clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────┐
│                        CLI / MCP                            │
│                   (User interaction layer)                  │
├─────────────────────────────────────────────────────────────┤
│     agent_mod    │   session_mod   │    team    │    ai     │
│   (Agent mgmt)   │   (Session mgmt) │  (Teams)  │  (AI API) │
├─────────────────────────────────────────────────────────────┤
│                      notification                           │
│                 (Multi-channel notifications)               │
├─────────────────────────────────────────────────────────────┤
│                        infra                                │
│              (tmux, process scanning, jsonl)                │
└─────────────────────────────────────────────────────────────┘
```

### OpenClaw Plugin Integration

```
OpenClaw Gateway → CAM Plugin (TypeScript) → cam serve (MCP) → Rust Backend
                        ↓
                  spawn + stdin/stdout
                        ↓
                  JSON-RPC 2.0 Protocol
```

The plugin calls `cam serve` as a subprocess, communicating via JSON-RPC over stdin/stdout.

### Module Structure

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Library exports
├── infra/               # Infrastructure layer
│   ├── tmux.rs          # tmux session management
│   ├── process.rs       # Process scanning
│   ├── jsonl.rs         # JSONL log parsing
│   └── input.rs         # Input wait detection
├── agent_mod/           # Agent lifecycle management
│   ├── manager.rs       # Start/stop/list agents
│   ├── watcher.rs       # State monitoring
│   ├── daemon.rs        # Background watcher daemon
│   └── adapter/         # Multi-CLI adapter layer
│       ├── mod.rs       # AgentAdapter trait
│       ├── types.rs     # HookEvent, DetectionStrategy
│       ├── claude.rs    # Claude Code adapter
│       ├── codex.rs     # Codex CLI adapter
│       ├── opencode.rs  # OpenCode adapter
│       ├── generic.rs   # Generic fallback adapter
│       └── config_manager.rs  # Config backup/restore
├── session_mod/         # Session management
│   ├── manager.rs       # Claude Code session listing
│   └── state.rs         # Conversation state, quick replies
├── mcp_mod/             # MCP Server
│   ├── server.rs        # JSON-RPC request handling
│   ├── types.rs         # Protocol types
│   └── tools/           # MCP tool implementations
├── notification/        # Notification system
│   ├── channel.rs       # NotificationChannel trait
│   ├── dispatcher.rs    # Multi-channel dispatcher
│   ├── urgency.rs       # Urgency classification
│   ├── formatter.rs     # AI-powered message formatting
│   ├── deduplicator.rs  # 120s deduplication window
│   └── channels/        # Telegram, WhatsApp, Dashboard, etc.
├── team/                # Agent Teams
│   ├── discovery.rs     # Team config discovery
│   ├── bridge.rs        # File system bridge
│   ├── orchestrator.rs  # Agent orchestration
│   ├── task_list.rs     # Task management
│   └── inbox_watcher.rs # Inbox monitoring
├── tui/                 # TUI Dashboard
│   ├── app.rs           # Application state and main loop
│   ├── event.rs         # Event handling (keyboard, tick)
│   ├── ui.rs            # UI rendering (dashboard, logs)
│   ├── logs.rs          # Log viewer with level filtering
│   ├── search.rs        # Lazygit-style search input
│   ├── state.rs         # Agent/notification state types
│   └── terminal_stream.rs # Real-time tmux capture
├── ai/                  # AI integration
│   ├── client.rs        # Anthropic API client
│   └── extractor.rs     # Terminal content extraction
└── anthropic.rs         # Haiku API convenience wrapper
```

### Architecture Documentation

For detailed architecture documentation, see:

- [Core Modules](docs/architecture/core-modules.md) - Module responsibilities and dependencies
- [Plugin Integration](docs/architecture/plugin-integration.md) - OpenClaw plugin architecture
- [Notification System](docs/architecture/notification-system.md) - Multi-channel notification routing
- [Agent Teams](docs/architecture/agent-teams.md) - Multi-agent collaboration system

### Notification Routing

All notifications are sent via Webhook to OpenClaw Gateway (`POST /hooks/agent`), which triggers an OpenClaw conversation. Users can reply directly in the conversation, and the CAM skill will process the reply via `cam reply`.

| Urgency | Events | Behavior |
|---------|--------|----------|
| HIGH | permission_request, Error, WaitingForInput | Send immediately, requires user response |
| MEDIUM | AgentExited, idle_prompt | Send notification, may need user action |
| LOW | session_start, stop, ToolUse | Silent (no notification sent) |

#### Auto-Approve (OpenClaw Skill)

OpenClaw can automatically approve low-risk operations using a three-layer decision model:

1. **Whitelist** - Safe commands auto-approved: `ls`, `cat`, `git status`, `cargo test`, `npm test`
2. **Blacklist** - Always require human confirmation: `rm`, `sudo`, commands with `&&`, `|`, `>`
3. **LLM Judgment** - AI analyzes risk for commands not in either list

**Parameter Safety Check**: Even whitelisted commands require confirmation if arguments contain sensitive paths (`/etc/`, `~/.ssh/`, `.env`).

See [Auto-Approve Design](docs/plans/2026-02-24-auto-approve-design.md) for details.

Reply flow:
```
CAM → POST /hooks/agent → Gateway → OpenClaw conversation
                                        ↓
                              User replies "y"
                                        ↓
                              CAM skill → cam reply → tmux send-keys
```

### Data Storage

| Path | Description |
|------|-------------|
| `~/.config/code-agent-monitor/agents.json` | Running agent records |
| `~/.config/code-agent-monitor/watcher.pid` | Watcher process PID |
| `~/.config/code-agent-monitor/hook.log` | Hook logs |
| `~/.config/code-agent-monitor/conversation_state.json` | Conversation state |
| `~/.config/code-agent-monitor/config.json` | Webhook and Haiku API configuration |
| `~/.config/code-agent-monitor/backups/` | Config file backups (created by `cam setup`) |
| `~/.config/code-agent-monitor/notifications.jsonl` | Local notification records for TUI |
| `~/Library/LaunchAgents/com.cam.watcher.plist` | launchd service config (macOS) |
| `~/.claude/teams/` | Agent Teams |
| `~/.claude/tasks/` | Task lists |

## Development

### Build

```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Run Tests

```bash
# Run all tests
cargo test

# Run tests sequentially (avoid tmux conflicts)
cargo test -- --test-threads=1

# Run specific module tests
cargo test --lib notification
cargo test --lib team
```

### Update Plugin Binary

```bash
cargo build --release
cp target/release/cam plugins/cam/bin/cam
openclaw gateway restart

# Restart service to load new binary
cam service restart
```

## License

MIT
