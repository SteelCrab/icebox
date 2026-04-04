# icebox-commands

Slash command definitions, parser, and autocomplete for the TUI chat input.

## Commands (17)

| Category | Commands |
|----------|----------|
| **Board** | `/new`, `/move`, `/delete`, `/search`, `/export`, `/diff` |
| **AI** | `/help`, `/status`, `/cost`, `/clear`, `/compact`, `/model`, `/remember`, `/memory` |
| **Auth** | `/login`, `/logout` |
| **Session** | `/resume` |

## API

- `SlashCommand::parse(input)` — parse `/command arg` string
- `filter_commands(prefix)` — match commands for suggestion popup
- `autocomplete(prefix)` — single best match for Tab completion
- `render_help()` — formatted help text grouped by category
