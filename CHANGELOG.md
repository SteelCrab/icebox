# Changelog

All notable changes to this project will be documented in this file.

## v0.3.1

Small CLI improvements.

### ✨ Features
- **`icebox web [args...]`** — launch the local web UI directly from the main CLI; delegates to `icebox-web` (sibling to current exe first, then PATH) and forwards args + exit code

### 🔧 Improvements
- Release workflow reads notes from `CHANGELOG.md` per tag (falls back to auto-generated)
- `actions/checkout` upgraded to v5 (Node.js 24)

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
Download from the [release assets](https://github.com/SteelCrab/icebox/releases/tag/v0.3.1).

### ⬆️ Upgrade

```bash
brew upgrade icebox
```

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.3.0...v0.3.1

## v0.3.0

Web UI, MCP server, Notion sync.

### ✨ Features
- **Web UI** — `icebox-web` crate with local kanban board in the browser
  - AI Chat panel (WebSocket streaming, tool execution, markdown rendering)
  - Drag-and-drop task movement between columns
  - Swimlane filter, model selector (Opus/Sonnet/Haiku), effort level
  - Resizable chat panel, responsive layout (desktop/tablet/mobile)
- **MCP Server** — `icebox mcp` for Claude Code integration (12 tools over JSON-RPC 2.0 stdio)
- **Notion Sync** — `/notion push` and `/notion pull` with bidirectional sync

### 🔧 Improvements
- Notion pull no longer deletes local tasks (safety fix)
- Secret files (`config.json`, `.env`) added to `.gitignore`

### 📚 Docs
- `docs/mcp.md` — MCP server setup guide
- `docs/web.md` — Web UI guideline

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
```bash
# x86_64
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.3.0/icebox-x86_64-unknown-linux-gnu.tar.gz
tar xzf icebox-x86_64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/

# aarch64
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.3.0/icebox-aarch64-unknown-linux-gnu.tar.gz
tar xzf icebox-aarch64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/
```

### ⬆️ Upgrade

**macOS**
```bash
brew upgrade icebox
```

**Linux**
Download the latest binary from the [release assets](https://github.com/SteelCrab/icebox/releases/tag/v0.3.0).

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.2.1...v0.3.0

## v0.2.1

Multi-arch release pipeline and bug fixes.

### 🔧 Improvements
- Multi-arch release pipeline (macOS arm64, Linux x86_64/aarch64/armv7, musl)
- Fix modal popup key input routing when bottom chat is focused

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
Download from the [release assets](https://github.com/SteelCrab/icebox/releases/tag/v0.2.1).

### ⬆️ Upgrade

**macOS**
```bash
brew upgrade icebox
```

**Linux**
Download the latest binary from the release assets.

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.2.0...v0.2.1

## v0.2.0

Swimlane filtering, new slash commands, easier install.

### ✨ Features
- Swimlane filtering — tag tasks and filter the board via the tab bar
- `/swimlane` (aliases: `/sl`, `/lane`), `]` / `[` to cycle, `s` to clear
- New board commands: `/new`, `/move`, `/delete`, `/search`, `/export`, `/diff`
- Multilingual READMEs: JA, ZH, ES

### 🔧 Improvements
- Bare `#N` no longer duplicates PR/issue links
- `list_tasks` returns full UUIDs for AI disambiguation
- Slash command routing fix

### 📚 Docs
- `docs/swimlane.md`

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux / From source**
```bash
cargo install --git https://github.com/SteelCrab/icebox.git
```

### ⬆️ Upgrade

**macOS**
```bash
brew upgrade icebox
```

**Linux / From source**
```bash
cargo install --git https://github.com/SteelCrab/icebox.git --force
```

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.1.0...v0.2.0

## v0.1.0

Rust TUI Kanban Board with integrated AI Sidebar.

### ✨ Features
- 5-column kanban board (Icebox, Emergency, In Progress, Testing, Complete)
- Per-task AI conversations with streaming responses (Anthropic API)
- 12 built-in AI tools (bash, file ops, code search, task management, memory)
- 17 slash commands
- OAuth PKCE authentication (claude.ai)
- Mouse support, text selection, vim-style keybindings
- Task dates (start/due), tags, priority levels
- Markdown task storage with YAML frontmatter

### 📦 Install

**From source**
```bash
cargo install --git https://github.com/SteelCrab/icebox.git
```
