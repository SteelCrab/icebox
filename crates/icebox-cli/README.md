# icebox

Main binary crate. CLI entry point with subcommands and TUI runtime launcher.

## Subcommands

| Command | Description |
|---------|-------------|
| `icebox` | Launch TUI kanban board |
| `icebox [path]` | Launch TUI at specific path |
| `icebox init` | Initialize `.icebox/` workspace |
| `icebox login` | OAuth PKCE authentication |
| `icebox logout` | Clear credentials |
| `icebox whoami` | Show auth status |

## Key Responsibilities

- CLI argument parsing and subcommand dispatch
- TUI terminal setup/teardown (`crossterm`)
- AI runtime thread spawn (tokio background thread)
- Session management (per-task session swap)
