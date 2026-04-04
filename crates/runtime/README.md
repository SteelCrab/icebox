# icebox-runtime

Conversation runtime, session persistence, OAuth PKCE flow, and usage tracking.

## Modules

| File | Description |
|------|-------------|
| `conversation.rs` | `ConversationRuntime` — streaming turn loop with tool execution |
| `session.rs` | `Session` — message history, disk save/load |
| `oauth.rs` | OAuth PKCE flow (browser + console) |
| `config.rs` | `IceboxConfig` — model selection, saved preferences |
| `usage.rs` | Token usage tracking and cost calculation |

## Architecture

```
TUI Thread                    Tokio Thread
  RuntimeCommand ──────────> ConversationRuntime::run_turn()
  AiEvent <──────────────── AnthropicClient::stream()
  ToolApproval ────────────> ToolExecutor::execute()
```

## Key Features

- Per-session message history with disk persistence
- Tool approval flow (Yes / Always / No)
- Auto-compact at 200K tokens
- Multi-session management (per-task + global)
