# Changelog

All notable changes to this project will be documented in this file.

## v0.5.0

Full Windows architecture support across runtime, tooling, and release pipeline.

### ✨ Features
- **Windows builds** in the release pipeline:
  - `x86_64-pc-windows-msvc`
  - `aarch64-pc-windows-msvc`
- Windows artifacts distributed as `.zip` archives
- Cross-platform home directory resolution (`HOME` on Unix, `USERPROFILE` on Windows)
- Shell execution via `cmd /C` on Windows, `sh -c` elsewhere
- **`icebox upgrade`** — self-update via `self_update` crate (macOS, Linux, Windows)

### 🔧 Improvements
- File permission handling guarded by `#[cfg(unix)]` (no-op on Windows)
- Release workflow runs on `windows-latest` for native Windows builds

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
```bash
# x86_64
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.5.0/icebox-x86_64-unknown-linux-gnu.tar.gz
tar xzf icebox-x86_64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/

# aarch64
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.5.0/icebox-aarch64-unknown-linux-gnu.tar.gz
tar xzf icebox-aarch64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/
```

**Windows (PowerShell)** — see [README](README.md#windows) for details
```powershell
$arch = if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'aarch64' } else { 'x86_64' }
Invoke-WebRequest "https://github.com/SteelCrab/icebox/releases/download/v0.5.0/icebox-$arch-pc-windows-msvc.zip" -OutFile icebox.zip
Expand-Archive icebox.zip "$env:USERPROFILE\icebox" -Force
$p = [Environment]::GetEnvironmentVariable('Path','User'); if (($p -split ';') -notcontains "$env:USERPROFILE\icebox") { [Environment]::SetEnvironmentVariable('Path', "$env:USERPROFILE\icebox;$p", 'User') }
```

### ⬆️ Upgrade

```bash
icebox upgrade         # self-update (any platform)
brew upgrade icebox    # macOS Homebrew
```

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.4.2...v0.5.0

## v0.4.2

Single-binary web UI — `icebox web` now runs in-process, no second binary required.

### 🔧 Improvements
- **`icebox web` runs in-process** — previously the subcommand delegated to a separate `icebox-web` binary via `Command::spawn`, which failed with `command not found` for users who only installed `icebox`. Now the local kanban web UI starts directly inside the `icebox` binary on its own Tokio runtime.
- `icebox-web` crate converted to **library-only**: `[[bin]]` target removed, `clap` dependency dropped, new public entry `icebox_web::serve(path, port)`.
- `icebox web --help` now documents `--path` and `--port` options.

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
```bash
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.4.2/icebox-x86_64-unknown-linux-gnu.tar.gz
tar xzf icebox-x86_64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/
```

### ⬆️ Upgrade

```bash
brew upgrade icebox
```

### Quick Start

```bash
icebox web --path . --port 3000
# → http://127.0.0.1:3000
```

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.4.1...v0.4.2

## v0.4.1

UX polish for `icebox init --all`.

### 🔧 Improvements
- **Memory content preview** — `init --all` now prints the exact file path and full memory entry that will be written to `~/.claude/projects/<slug>/memory/project_icebox_workflow.md` **before** asking for Y/n confirmation, so users can review what goes into Claude Code memory before accepting.

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
```bash
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.4.1/icebox-x86_64-unknown-linux-gnu.tar.gz
tar xzf icebox-x86_64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/
```

### ⬆️ Upgrade

```bash
brew upgrade icebox
```

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.4.0...v0.4.1

## v0.4.0

CLI ergonomics: one-shot workspace setup, integrated web launcher, self-update, Windows support.

### ✨ Features
- **`icebox init --all`** — one command to set up `.icebox/`, `.mcp.json`, and Claude Code memory (with Y/n prompts, skips if already exists)
- **`icebox web`** — launch the local web UI directly from the CLI (delegates to `icebox-web`, forwards args and exit code)
- **`icebox upgrade`** — self-update the binary from the latest GitHub release
- **Windows support** — `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` in the release pipeline (ZIP archives)

### 🔧 Improvements
- Release workflow reads notes from `CHANGELOG.md` per tag (falls back to auto-generated)
- `actions/checkout` upgraded to v5 (Node.js 24)
- Cross-platform HOME resolution (HOME → USERPROFILE)
- Shell execution switches to `cmd /C` on Windows, `sh -c` elsewhere
- Minimalist init output (`  created  .icebox/`)

### 📦 Install

**macOS (Homebrew)**
```bash
brew tap SteelCrab/tap && brew install icebox
```

**Linux**
```bash
curl -LO https://github.com/SteelCrab/icebox/releases/download/v0.4.0/icebox-x86_64-unknown-linux-gnu.tar.gz
tar xzf icebox-x86_64-unknown-linux-gnu.tar.gz && mv icebox ~/.local/bin/
```

### ⬆️ Upgrade

```bash
brew upgrade icebox
```

### Quick Start

```bash
cd your-project
icebox init --all      # ★ recommended — workspace + MCP + memory
```

**Full Changelog**: https://github.com/SteelCrab/icebox/compare/v0.3.1...v0.4.0

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
