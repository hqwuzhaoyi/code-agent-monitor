# Code Agent Monitor (CAM)

[中文](README.zh-CN.md)

Monitor and manage AI coding agents remotely through OpenClaw.

AI coding agents frequently need human input — permission confirmations, decisions, error handling. Without CAM, you have to sit at your computer watching a terminal. With CAM, you handle all of that from your phone through OpenClaw conversations.

```
Agent needs input → CAM detects → OpenClaw notifies you → You reply → Agent continues
```

## Features

- **Remote monitoring** — Get notified on your phone via OpenClaw when agents need input
- **TUI dashboard** — Four-panel terminal UI: agent list, terminal preview, notifications, details
- **Multi-agent support** — Works with Claude Code, Codex, and OpenCode
- **AI-powered extraction** — Uses AI to understand what agents are asking
- **Risk assessment** — Classifies permission requests as Low/Medium/High risk
- **Agent Teams** — Orchestrate multiple agents working on the same project
- **Smart deduplication** — 120-second window with 80% similarity matching prevents notification spam
- **Always-on service** — Runs as a launchd service on macOS for background monitoring

## Getting Started

This guide takes you from zero to a working setup in about 10 minutes.

### Prerequisites

Before you begin, make sure you have:

- **Rust toolchain** — Install from [rustup.rs](https://rustup.rs) if you don't have it
- **tmux** — `brew install tmux` on macOS
- **OpenClaw** — Installed and configured with a messaging channel (this is how you receive notifications)
- **An Anthropic API key** (recommended) — CAM uses Claude Haiku for AI-powered terminal analysis

### Step 1: Install CAM

```bash
git clone https://github.com/anthropics/code-agent-monitor.git
cd code-agent-monitor
cargo build --release
```

Add the binary to your PATH:

```bash
cp target/release/cam /usr/local/bin/
```

If you use OpenClaw, install the plugin and skills:

```bash
# Install CAM plugin
openclaw plugins install --link /path/to/code-agent-monitor/plugins/cam

# Install skills
mkdir -p ~/.openclaw/skills
for skill in cam agent-teams cam-notify; do
  cp -r skills/$skill ~/.openclaw/skills/
done

openclaw gateway restart
```

Verify the installation:

```bash
cam --help
```

### Step 2: Bootstrap Configuration (Recommended)

Run the interactive setup wizard — it auto-detects your OpenClaw config and installed agent tools:

```bash
cam bootstrap
```

This configures webhook, AI monitoring, and agent hooks in one step. If you have OpenClaw installed, it will automatically detect your gateway URL, hook token, and API providers.

For fully automated setup (no prompts, uses all detected defaults):

```bash
cam bootstrap --auto
```

If you prefer to configure each piece manually, follow Steps 2a and 2b below. Otherwise, skip to Step 3.

### Step 2a: Configure Webhook (Manual)

CAM sends notifications to OpenClaw Gateway via webhook. Create the config directory and file:

```bash
mkdir -p ~/.config/code-agent-monitor
```

Create `~/.config/code-agent-monitor/config.json`:

```json
{
  "webhook": {
    "gateway_url": "http://localhost:18789",
    "hook_token": "your-token",
    "timeout_secs": 30
  },
  "anthropic_api_key": "sk-ant-..."
}
```

Replace `your-token` with your OpenClaw hook token, and `sk-ant-...` with your Anthropic API key (recommended for AI-powered monitoring).

> **Where do I get these values?**
> - `gateway_url`: The address of your OpenClaw Gateway (default is `http://localhost:18789`)
> - `hook_token`: The `hooks.token` value from your OpenClaw config (`~/.openclaw/openclaw.json`). You can find it with:
>   ```bash
>   cat ~/.openclaw/openclaw.json | python3 -c "import sys,json; print(json.load(sys.stdin)['hooks']['token'])"
>   ```
> - `anthropic_api_key`: Your Anthropic API key for Claude Haiku — powers AI-driven terminal analysis and smart notification extraction. Strongly recommended; without it, notifications will lack AI analysis capabilities

### Step 2b: Set Up Agent Hooks (Manual)

This tells Claude Code to notify CAM on events like permission requests and idle prompts:

```bash
cam setup claude
```

You can preview what changes will be made before applying:

```bash
cam setup --dry-run claude
```

For other agents:

```bash
cam setup codex      # Codex CLI
cam setup opencode   # OpenCode
```

### Step 3: Install the Watcher Service

The watcher runs in the background, continuously monitoring your agents' terminal sessions:

```bash
cam install
```

Verify it's running:

```bash
cam service status
```

You should see the service reported as active. If you ever need to restart it:

```bash
cam service restart
```

### Step 4: Start Your First Agent

Everything is set up! Open your OpenClaw conversation and start an agent using natural language:

```
You: Start a Claude in ~/workspace/myapp
OpenClaw: Starting...
          ✅ Started Claude @ ~/workspace/myapp (cam-1706789012)
```

You can also give the agent a task right away:

```
You: Start Claude in ~/workspace/myapp, implement a TODO app
```

More natural language examples:

| What you say | What happens |
|-------------|-------------|
| "Start a Claude in /path/to/project" | Launches a Claude Code agent |
| "Start Codex on my-project" | Launches a Codex agent |
| "What's running?" / "Status" | Lists running agents |
| "Continue" / "y" / "go ahead" | Sends confirmation to agent |
| "Show output" / "How's it going?" | Shows agent logs |
| "Stop it" / "Cancel" | Stops the agent |
| "Resume the last one" | Resumes a previous session |

> You can also use the CLI directly: `cam start "Implement a TODO app"`

### Step 5: Open the TUI Dashboard

```bash
cam tui
```

You'll see a four-panel dashboard showing your running agents, a live terminal preview, and notification history. Use `Tab` to switch between panels and `j`/`k` to navigate.

### Step 6: Receive Notifications

When your agent hits a permission request, encounters an error, or asks a question, CAM detects it and sends a notification through OpenClaw. You'll receive a message on your phone with the context of what the agent needs.

Reply directly in the OpenClaw conversation:
- Type `y` to approve a permission request
- Type `n` to reject
- Type any text to answer an open-ended question

That's it. Your agent continues working, and you didn't have to touch your computer.

## TUI Dashboard

The TUI provides a real-time view of all your running agents.

```
┌─────────────────────┬──────────────────────────┐
│   Agent List        │  Terminal Preview         │
│                     │  (live tmux capture)      │
├─────────────────────┼──────────────────────────┤
│   Notifications     │  Notification Detail      │
│                     │  (project, risk, context) │
├─────────────────────┴──────────────────────────┤
│   Help Bar (context-sensitive shortcuts)        │
└─────────────────────────────────────────────────┘
```

- **Agent List** (left) — All running agents with status indicators (Running/Idle/Error)
- **Terminal Preview** (top-right) — Live scrollable capture of the selected agent's tmux session
- **Notifications** (bottom-left) — History with urgency color coding (red/yellow/gray)
- **Detail View** (bottom-right) — Full notification context: project, risk level, terminal snapshot

Key bindings:

| Key | Action |
|-----|--------|
| `Tab` | Switch focus between panels |
| `j` / `k` | Navigate items in focused panel |
| `Enter` | Attach to selected agent's tmux session |
| `x` / `d` | Close selected agent |
| `/` | Filter by agent ID or project name |
| `l` | Switch to logs view |
| `Esc` | Clear filter / return to agent list |
| `q` | Quit |

## CLI Reference

### Agent Management

| Command | Description |
|---------|-------------|
| `cam start [prompt]` | Start a new agent (optionally with an initial prompt) |
| `cam list` | List all running agents |
| `cam kill <pid>` | Kill an agent process |
| `cam resume <session_id>` | Attach to an agent's tmux session |
| `cam sessions` | List historical sessions |
| `cam logs <session_id>` | View session logs |

### Monitoring

| Command | Description |
|---------|-------------|
| `cam tui` | Launch the TUI dashboard |
| `cam watch-daemon` | Start the background watcher manually |
| `cam setup <agent>` | Configure hooks for an agent CLI |

### Notifications

| Command | Description |
|---------|-------------|
| `cam notify --event <event>` | Send a notification event |
| `cam watch-trigger --agent-id <id>` | Manually trigger detection (debugging) |
| `cam pending-confirmations` | View pending permission requests |
| `cam reply <response>` | Reply to a pending request |
| `cam reply y --all` | Approve all pending requests |
| `cam reply y --risk low` | Approve all low-risk requests |

### Service Management

| Command | Description |
|---------|-------------|
| `cam install` | Install watcher as a launchd service |
| `cam uninstall` | Remove the launchd service |
| `cam service status` | Check service status |
| `cam service restart` | Restart the service |
| `cam service logs [-f]` | View (or follow) service logs |

### Teams

| Command | Description |
|---------|-------------|
| `cam team-create <name>` | Create a new agent team |
| `cam team-spawn <team> <name>` | Add an agent to a team |
| `cam team-progress <team>` | View team task progress |
| `cam team-shutdown <team>` | Shut down all agents in a team |

## Notification System

CAM classifies events by urgency and only notifies you when it matters:

| Urgency | Events | Behavior |
|---------|--------|----------|
| HIGH | Permission requests, errors, waiting for input | Sent immediately — requires your response |
| MEDIUM | Agent exited, idle prompt | Sent as notification — may need action |
| LOW | Session start/stop, tool use | Silent — logged locally only |

Notifications are powered by AI analysis, which reads terminal snapshots to extract the actual question or context the agent is presenting. A 120-second deduplication window with 80% similarity matching prevents repeated notifications for the same event.

For permission requests, CAM assesses risk level (Low/Medium/High) based on the command being executed. OpenClaw can auto-approve low-risk commands like `ls`, `cat`, and `git status`, while flagging destructive commands like `rm` or `sudo` for manual review.

## Configuration

All configuration lives in `~/.config/code-agent-monitor/`:

| File | Purpose |
|------|---------|
| `config.json` | Webhook URL, API keys, and AI monitoring configuration |
| `agents.json` | Currently running agent records |
| `notifications.jsonl` | Local notification history (used by TUI) |
| `dedup_state.json` | Notification deduplication state |
| `hook.log` | Webhook delivery log |

The Anthropic API key (for AI monitoring) can also be provided via:
1. `ANTHROPIC_API_KEY` environment variable
2. `~/.anthropic/api_key` file
3. `~/.openclaw/openclaw.json`

## Architecture

```
Claude Code Hooks ──→ CAM ──→ Webhook ──→ OpenClaw Gateway ──→ Your Phone
Terminal Watcher  ──↗        ↙
                    CAM TUI
```

CAM sits between your AI coding agents and OpenClaw. It monitors agents through two channels:

1. **Hook events** — Claude Code/Codex/OpenCode send events directly to CAM via configured hooks
2. **Terminal watcher** — A background service polls tmux sessions and uses AI to detect when agents are waiting for input

Both channels feed into the notification system, which deduplicates, classifies urgency, and forwards to OpenClaw Gateway.

For detailed architecture docs, see [docs/architecture/](docs/architecture/).

## Development

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run tests sequentially (avoids tmux conflicts)
cargo test -- --test-threads=1

# Update the plugin binary after changes
cargo build --release
cp target/release/cam plugins/cam/bin/cam
cam service restart
```

See [docs/development.md](docs/development.md) for project structure and contribution guidelines.

## License

MIT
