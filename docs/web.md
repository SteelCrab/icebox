# Web UI Guide

> A lightweight local web server that renders your icebox board in the browser.
> Reads the same `.icebox/tasks/` files as the TUI — no separate data store.

## Quick Start

```bash
# From source
cargo run -p icebox-web

# Release build
cargo build --release -p icebox-web
./target/release/icebox-web
```

Then open **http://localhost:3000** in your browser.

## Options

| Flag | Description | Default |
|------|-------------|---------|
| `--path <PATH>` | Workspace directory (must contain `.icebox/`) | `.` |
| `--port <PORT>` | Port to listen on | `3000` |

```bash
icebox-web --path ./my-board --port 8080
```

## Responsive Layout

The board adapts to the viewport automatically — no configuration needed.

| Viewport | Layout |
|----------|--------|
| ≥ 900 px | 5-column horizontal kanban (all columns visible side-by-side) |
| 600–899 px | Tab bar at the top — tap a column name to switch |
| < 600 px | Tab bar + modal slides up from the bottom of the screen |

## Card Detail

Click any card to open a detail view showing:

- Title and priority
- Column and swimlane
- Created, start, and due dates
- Progress (`done / total`) if set
- Tags
- Full task body (markdown source)

Press `Esc` or click the backdrop to close.

## Architecture

`icebox-web` is a standalone Axum HTTP server (`crates/icebox-web/`).

| Route | Description |
|-------|-------------|
| `GET /` | Serves the single-page HTML app (embedded at compile time) |
| `GET /api/tasks` | Returns all tasks as a JSON array |

The frontend is plain HTML + CSS + vanilla JS with no build step or external dependencies. It polls `/api/tasks` on load and on each **Refresh** click.

## Task Data

Tasks are read directly from `.icebox/tasks/*.md` — the same markdown files
the TUI writes. Changes made in the TUI are visible after pressing **Refresh**
in the browser.

See [Task Storage](../README.md#task-storage) for the file format.

## Building from Source

```bash
git clone https://github.com/SteelCrab/icebox.git
cd icebox
cargo build --release -p icebox-web
cp target/release/icebox-web ~/.local/bin/
```

Requires Rust edition 2024 (`rustup update stable`).

## References

- icebox repository: https://github.com/SteelCrab/icebox
- Related: [Swimlane Guide](./swimlane.md)
