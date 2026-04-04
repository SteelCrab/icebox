# icebox-tools

Built-in AI tool executor. Implements `ToolExecutor` trait for the conversation runtime.

## Tools (12)

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands |
| `read_file` | Read file contents |
| `write_file` | Write/create files |
| `glob_search` | Find files by glob pattern |
| `grep_search` | Search file contents with regex |
| `list_tasks` | List kanban tasks by column |
| `create_task` | Create a new task |
| `update_task` | Update existing task fields |
| `move_task` | Move task to another column |
| `save_memory` | Save persistent memory |
| `list_memories` | List saved memories |
| `delete_memory` | Delete a memory entry |

## Tool Name Normalization

Models sometimes hallucinate prefixes (`mcp_bash`, `icebox_read_file`). The executor strips `mcp_` and `icebox_` prefixes before dispatch.
