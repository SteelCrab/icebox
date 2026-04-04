# icebox-api

Anthropic API client with SSE streaming, retry logic, and OAuth support.

## Modules

| File | Description |
|------|-------------|
| `client.rs` | `AnthropicClient` — HTTP client with retry, auth plugins |
| `types.rs` | API request/response types (`MessageRequest`, `StreamEvent`, etc.) |
| `error.rs` | Error types with `is_retryable()` classification |
| `sse.rs` | Server-Sent Events parser |
| `oauth_transform.rs` | OAuth header transforms and beta flag management |

## Auth Methods

- `AuthMethod::ApiKey` — `x-api-key` header
- `AuthMethod::Bearer` — OAuth Bearer token with required headers
- `AuthMethod::Combined` — Both (proxy use)

## Features

- 5-retry with exponential backoff (2s, 4s, 8s, 16s, 60s)
- `Retry-After` header support
- SSE streaming for real-time token output
