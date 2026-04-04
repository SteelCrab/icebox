# icebox-tui

TUI rendering and input handling built on `ratatui` + `crossterm`.

## Modules

| File | Description |
|------|-------------|
| `app.rs` | `App` — main state, event loop, rendering dispatch |
| `board.rs` | `BoardState` — column/task selection state |
| `column.rs` | Column rendering (task list per column) |
| `card.rs` | Card rendering (priority, tags, dates, meta line) |
| `sidebar.rs` | Task detail + AI chat rendering, markdown renderer |
| `input.rs` | Keyboard/mouse event handlers per mode |
| `layout.rs` | Layout computation (board, sidebar, bottom chat) |
| `theme.rs` | Color scheme and style definitions |

## Modes

| Mode | Description |
|------|-------------|
| `Board` | Main kanban view, navigate columns/tasks |
| `TaskDetail` | Sidebar with task info + AI chat |
| `EditTask` | Edit task title/body |
| `CreateTask` | New task modal (title, tags, dates) |
| `ConfirmDelete` | Delete confirmation |
| `SelectModel` | AI model picker |
| `Memory` | Memory management view |
