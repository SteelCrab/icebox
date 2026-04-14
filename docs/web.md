# Web UI Guideline

> Use the icebox kanban board in the browser.

## Quick Start

```bash
cargo run -p icebox-web              # dev build
cargo build --release -p icebox-web  # release build
./target/release/icebox-web
```

Open **http://localhost:3000**.

## Options

| Flag | Description | Default |
|------|-------------|---------|
| `--path <PATH>` | Workspace directory containing `.icebox/` | `.` |
| `--port <PORT>` | Port to listen on | `3000` |

```bash
icebox-web --path ./my-board --port 8080
```

## Board

- **Desktop** (900px+): All 5 columns side-by-side
- **Tablet** (600-899px): Tab bar to switch columns
- **Mobile** (<600px): Tab bar + bottom-sheet modal

## Card Detail

Click a card to open its detail modal:

- Title, priority, column, swimlane
- Created, start, due dates, progress
- Tags
- Body (rendered as markdown)

Press `Esc` or click the backdrop to close.

## Moving Tasks

Drag a card and drop it into another column. The board refreshes automatically.

## Swimlane Filter

Use the dropdown in the top-right corner:

- **All**: Show every task
- **@name**: Show only tasks in that swimlane

## AI Chat

Click the **AI Chat** button (bottom-right) to open the chat panel.

### Authentication

Set one of the following:

```bash
export ANTHROPIC_API_KEY=sk-...   # API key (recommended)
icebox login                      # OAuth login
```

### Usage

1. Type a message and press `Enter` or click **Send**
2. Responses stream in real-time
3. The board refreshes automatically after each AI turn

### Switching Models

Use the model dropdown in the chat header:

| Model | Best for |
|-------|----------|
| Opus | Complex work |
| Sonnet | Everyday tasks |
| Haiku | Quick answers |

### Effort Level

Select effort from the dropdown next to the model (Low / Medium / High / Max).

### Resizing the Panel

Drag the handle (horizontal bar) at the top of the chat panel up or down.

### Clearing the Session

Click the **Clear** button to reset conversation history.

## Data

Reads and writes the same `.icebox/tasks/*.md` files as the TUI.
TUI changes appear after clicking **Refresh**. Web changes appear in the TUI immediately.

## Building from Source

```bash
git clone https://github.com/SteelCrab/icebox.git
cd icebox
cargo build --release -p icebox-web
cp target/release/icebox-web ~/.local/bin/
```

Requires Rust edition 2024 (`rustup update stable`).
