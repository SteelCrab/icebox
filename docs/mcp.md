# MCP Server Guide

> Connect icebox to Claude Code so the AI can manage your kanban board directly.

## Prerequisites

- icebox v0.3.0 or later (`icebox --version` to check)

## Quick Start

### 01. Create `.mcp.json` in your project root

```json
{
  "mcpServers": {
    "icebox": {
      "command": "icebox",
      "args": ["mcp"]
    }
  }
}
```

### 02. Initialize the workspace (if not already done)

```bash
icebox init
```

This creates the `.icebox/tasks/` directory where tasks are stored.

### 03. Verify the connection

Restart Claude Code in this project. You should see `icebox` listed as an MCP server. Ask Claude:

> "List my kanban tasks."

Claude will call the `list_tasks` tool and show your board.

## Global Setup

To make icebox available in every project, add it to `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "icebox": {
      "command": "icebox",
      "args": ["mcp", "--workspace", "/path/to/your/project"]
    }
  }
}
```

Replace `/path/to/your/project` with the directory containing `.icebox/`.

## Available Tools

| Tool | What it does |
|------|-------------|
| `list_tasks` | List all tasks grouped by column |
| `create_task` | Create a new task |
| `update_task` | Update title, priority, tags, dates, body |
| `move_task` | Move a task to another column |
| `bash` | Run a shell command |
| `read_file` | Read a file |
| `write_file` | Write a file |
| `glob_search` | Find files by glob pattern |
| `grep_search` | Search file contents with regex |
| `save_memory` | Save persistent AI context |
| `list_memories` | List saved memories |
| `delete_memory` | Delete a memory |

## Manual Testing

You can test the MCP server directly from the terminal:

```bash
# Initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | icebox mcp

# List available tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | icebox mcp

# List tasks
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_tasks","arguments":{}}}' | icebox mcp

# Create a task
echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"create_task","arguments":{"title":"Test task","column":"icebox"}}}' | icebox mcp
```

## Troubleshooting

**Claude Code doesn't see icebox tools**
- Check that `icebox` is in your PATH: `which icebox`
- Check that `.mcp.json` is in the project root (not a subdirectory)
- Restart Claude Code after adding `.mcp.json`

**"No workspace found" error**
- Run `icebox init` in the project directory first
- Or pass `--workspace /path/to/project` in the args

**Tools return empty results**
- Verify tasks exist: `ls .icebox/tasks/`
- Create a test task: `echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_task","arguments":{"title":"Hello"}}}' | icebox mcp`
