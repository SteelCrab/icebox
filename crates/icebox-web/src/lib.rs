use anyhow::Result;
use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use icebox_task::model::Column;
use icebox_task::store::TaskStore;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct AppState {
    store: TaskStore,
    workspace: PathBuf,
    shared_store: Arc<Mutex<TaskStore>>,
}

/// Start the Icebox local web UI server.
///
/// Serves the kanban board at `http://127.0.0.1:{port}` with the given
/// workspace directory (which must contain or be usable as `.icebox/`).
pub async fn serve(path: PathBuf, port: u16) -> Result<()> {
    let workspace = path.canonicalize().unwrap_or(path);
    let store = TaskStore::open(&workspace)?;
    let shared_store = Arc::new(Mutex::new(TaskStore::open(&workspace)?));
    let state = Arc::new(AppState {
        store,
        workspace,
        shared_store,
    });

    let app = Router::new()
        .route("/", get(serve_html))
        .route("/api/tasks", get(api_tasks))
        .route("/api/tasks/move", post(api_move_task))
        .route("/api/models", get(api_models))
        .route("/ws/chat", get(ws_chat_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    // Resolve the actual port — when `port == 0` the OS picks a free one,
    // so the printed URL must come from `local_addr()`, not the input.
    let bound = listener.local_addr()?;
    println!("icebox web → http://{bound}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_html() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    (StatusCode::OK, headers, HTML)
}

async fn api_tasks(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.list() {
        Ok(tasks) => {
            // Task.body is #[serde(skip)], so we inject it manually
            let values: Vec<serde_json::Value> = tasks
                .iter()
                .map(|t| {
                    let mut v = serde_json::to_value(t).unwrap_or_default();
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("body".to_string(), serde_json::Value::String(t.body.clone()));
                    }
                    v
                })
                .collect();
            let json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".into());
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, json)
        }
        Err(e) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                headers,
                format!(r#"{{"error":"{}"}}"#, e),
            )
        }
    }
}

// ── Move Task API ──

#[derive(Deserialize)]
struct MoveTaskRequest {
    task_id: String,
    column: String,
}

async fn api_move_task(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MoveTaskRequest>,
) -> impl IntoResponse {
    let column = match body.column.as_str() {
        "icebox" => Column::Icebox,
        "emergency" => Column::Emergency,
        "inprogress" => Column::InProgress,
        "testing" => Column::Testing,
        "complete" => Column::Complete,
        other => {
            return (
                StatusCode::BAD_REQUEST,
                format!(r#"{{"error":"unknown column: {other}"}}"#),
            );
        }
    };

    let store = match state.shared_store.lock() {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(r#"{{"error":"lock error: {e}"}}"#),
            );
        }
    };

    let mut task = match store.get(&body.task_id) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                format!(r#"{{"error":"task not found: {e}"}}"#),
            );
        }
    };

    task.column = column;
    task.updated_at = chrono::Utc::now();

    match store.save(&task) {
        Ok(()) => (StatusCode::OK, r#"{"ok":true}"#.to_string()),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(r#"{{"error":"save failed: {e}"}}"#),
        ),
    }
}

// ── Models API ──

async fn api_models() -> impl IntoResponse {
    let models: Vec<serde_json::Value> = icebox_runtime::MODELS
        .iter()
        .map(|m| {
            serde_json::json!({
                "alias": m.alias,
                "display_name": m.display_name,
                "id": m.id,
                "description": m.description
            })
        })
        .collect();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    (StatusCode::OK, headers, serde_json::to_string(&models).unwrap_or_else(|_| "[]".into()))
}

// ── WebSocket Chat ──

async fn ws_chat_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state))
}

async fn handle_ws_connection(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Resolve auth
    let resolved_auth = match icebox_runtime::AuthSource::resolve() {
        Ok(auth) => auth,
        Err(e) => {
            let msg = serde_json::json!({
                "type": "error",
                "message": format!("Auth error: {e}. Set ANTHROPIC_API_KEY or run `icebox login`.")
            });
            let _ = ws_tx.send(Message::Text(msg.to_string())).await;
            return;
        }
    };

    let is_oauth = matches!(
        &resolved_auth,
        icebox_runtime::AuthSource::BearerToken(_)
            | icebox_runtime::AuthSource::ApiKeyAndBearer { .. }
    );

    let client = match resolved_auth {
        icebox_runtime::AuthSource::ApiKey(key) => icebox_api::AnthropicClient::new(key),
        icebox_runtime::AuthSource::BearerToken(token) => {
            icebox_api::AnthropicClient::from_bearer(token)
        }
        icebox_runtime::AuthSource::ApiKeyAndBearer {
            api_key,
            bearer_token,
        } => icebox_api::AnthropicClient::from_combined(api_key, bearer_token),
        icebox_runtime::AuthSource::None => {
            let msg = serde_json::json!({
                "type": "status",
                "ai_available": false,
                "model": ""
            });
            let _ = ws_tx.send(Message::Text(msg.to_string())).await;
            let msg = serde_json::json!({
                "type": "error",
                "message": "No API key configured. Set ANTHROPIC_API_KEY or run `icebox login`."
            });
            let _ = ws_tx.send(Message::Text(msg.to_string())).await;
            return;
        }
    };

    // Resolve model (same priority as TUI: env > config > auth-aware default)
    let model = match std::env::var("ANTHROPIC_MODEL") {
        Ok(m) if !m.is_empty() => {
            icebox_runtime::resolve_model(&m).map_or(m, |info| info.id.to_string())
        }
        _ => icebox_runtime::IceboxConfig::saved_model()
            .and_then(|m| icebox_runtime::resolve_model(&m).map(|info| info.id.to_string()))
            .unwrap_or_else(|| icebox_runtime::default_model_for_auth(is_oauth).to_string()),
    };

    // Create tools
    let tools = match icebox_tools::IceboxToolExecutor::new(
        state.workspace.clone(),
        Arc::clone(&state.shared_store),
    ) {
        Ok(t) => Arc::new(t),
        Err(e) => {
            let msg = serde_json::json!({
                "type": "error",
                "message": format!("Failed to init tools: {e}")
            });
            let _ = ws_tx.send(Message::Text(msg.to_string())).await;
            return;
        }
    };

    // Create runtime
    let mut runtime = icebox_runtime::ConversationRuntime::new(client, model.clone());
    runtime.set_system_prompt(
        "You are an AI assistant integrated into the Icebox kanban board (web UI). \
         Help manage tasks, write code, search files, and execute commands. \
         Be concise and helpful. Answer in the user's language.\n\
         \n\
         Available tools:\n\
         - bash: Execute shell commands\n\
         - read_file: Read file contents\n\
         - write_file: Write/create files\n\
         - glob_search: Find files by glob pattern\n\
         - grep_search: Search file contents with regex\n\
         - list_tasks: List all kanban tasks by column\n\
         - create_task: Create a new task\n\
         - update_task: Update an existing task\n\
         - move_task: Move a task to another column\n\
         - save_memory / list_memories / delete_memory: Persistent memory\n\
         \n\
         CRITICAL: Use ONLY these exact tool names. NEVER add prefixes like 'mcp_' or 'icebox_'.",
    );

    // Load session from disk
    let session_key = "__web_global__";
    let session_path = icebox_runtime::session_path(&state.workspace, session_key);
    if let Ok(session) = icebox_runtime::Session::load_from_path(&session_path) {
        let _ = runtime.swap_session(session);
    }

    // Send status
    let status_msg = serde_json::json!({
        "type": "status",
        "ai_available": true,
        "model": model
    });
    let _ = ws_tx.send(Message::Text(status_msg.to_string())).await;

    // Event channel: runtime → ws sender
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<icebox_runtime::AiEvent>();

    // Dummy approval channel (auto_approve = true, so never used)
    let (_approval_tx, mut approval_rx) = tokio::sync::mpsc::unbounded_channel::<icebox_runtime::ToolApproval>();
    let mut auto_approve = true;

    // Shared flag: is a turn currently running?
    let busy = Arc::new(tokio::sync::Mutex::new(false));
    let busy_for_recv = Arc::clone(&busy);

    // Wrap ws_tx in Arc<Mutex> so both tasks can use it
    let ws_tx = Arc::new(tokio::sync::Mutex::new(ws_tx));
    let ws_tx_for_events = Arc::clone(&ws_tx);

    // Task 1: Forward AiEvents to WebSocket
    let event_forwarder = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let json = match &event {
                icebox_runtime::AiEvent::TextDelta(text) => {
                    serde_json::json!({ "type": "text_delta", "text": text })
                }
                icebox_runtime::AiEvent::ToolCallStart { name, input } => {
                    serde_json::json!({ "type": "tool_start", "name": name, "input": input })
                }
                icebox_runtime::AiEvent::ToolCallEnd {
                    name,
                    output,
                    is_error,
                } => {
                    serde_json::json!({ "type": "tool_end", "name": name, "output": output, "is_error": is_error })
                }
                icebox_runtime::AiEvent::TurnComplete => {
                    serde_json::json!({ "type": "turn_complete" })
                }
                icebox_runtime::AiEvent::Error(msg) => {
                    serde_json::json!({ "type": "error", "message": msg })
                }
                icebox_runtime::AiEvent::Usage(usage) => {
                    serde_json::json!({
                        "type": "usage",
                        "input_tokens": usage.input_tokens,
                        "output_tokens": usage.output_tokens
                    })
                }
                icebox_runtime::AiEvent::SessionContext { .. }
                | icebox_runtime::AiEvent::ToolApprovalRequest { .. } => continue,
            };
            let mut tx = ws_tx_for_events.lock().await;
            if tx.send(Message::Text(json.to_string())).await.is_err() {
                break;
            }
        }
    });

    // Task 2: Receive client messages, run turns
    let workspace_for_save = state.workspace.clone();
    while let Some(Ok(msg)) = ws_rx.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "send_message" => {
                let user_text = match parsed.get("text").and_then(|v| v.as_str()) {
                    Some(t) if !t.trim().is_empty() => t.to_string(),
                    _ => continue,
                };

                // Check if already busy
                {
                    let mut is_busy = busy_for_recv.lock().await;
                    if *is_busy {
                        let err = serde_json::json!({
                            "type": "error",
                            "message": "AI is still processing. Please wait."
                        });
                        let mut tx = ws_tx.lock().await;
                        let _ = tx.send(Message::Text(err.to_string())).await;
                        continue;
                    }
                    *is_busy = true;
                }

                match runtime
                    .run_turn(
                        &user_text,
                        tools.as_ref(),
                        &event_tx,
                        &mut approval_rx,
                        &mut auto_approve,
                    )
                    .await
                {
                    Ok(_usage) => {}
                    Err(e) => {
                        let _ = event_tx.send(icebox_runtime::AiEvent::Error(e.to_string()));
                        let _ = event_tx.send(icebox_runtime::AiEvent::TurnComplete);
                    }
                }

                // Save session to disk
                let save_path = icebox_runtime::session_path(&workspace_for_save, session_key);
                let _ = runtime.session().save_to_path(&save_path);

                *busy_for_recv.lock().await = false;
            }
            "clear_session" => {
                let _ = runtime.swap_session(icebox_runtime::Session::new());
                let save_path = icebox_runtime::session_path(&workspace_for_save, session_key);
                let _ = runtime.session().save_to_path(&save_path);
                let ack = serde_json::json!({ "type": "session_cleared" });
                let mut tx = ws_tx.lock().await;
                let _ = tx.send(Message::Text(ack.to_string())).await;
            }
            "switch_model" => {
                let alias = match parsed.get("model").and_then(|v| v.as_str()) {
                    Some(a) if !a.is_empty() => a,
                    _ => continue,
                };
                let model_id = icebox_runtime::resolve_model(alias)
                    .map_or_else(|| alias.to_string(), |info| info.id.to_string());
                runtime.set_model(&model_id);
                let ack = serde_json::json!({
                    "type": "status",
                    "ai_available": true,
                    "model": model_id
                });
                let mut tx = ws_tx.lock().await;
                let _ = tx.send(Message::Text(ack.to_string())).await;
            }
            _ => {}
        }
    }

    event_forwarder.abort();
}

static HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Icebox</title>
<style>
  *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

  :root {
    --bg: #fafafa;
    --surface: #ffffff;
    --border: #e5e5e5;
    --text: #111111;
    --muted: #888888;
    --priority-critical: #ef4444;
    --priority-high: #f59e0b;
    font-size: 13px;
  }

  body {
    background: var(--bg);
    color: var(--text);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Header ── */
  header {
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--surface);
    flex-shrink: 0;
  }

  header h1 {
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .subtitle {
    font-size: 12px;
    color: var(--muted);
  }

  .spacer { flex: 1; }

  .swimlane-select {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 4px 8px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    outline: none;
  }
  .swimlane-select:focus { border-color: #999; }

  .refresh-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 5px 10px;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
    white-space: nowrap;
  }
  .refresh-btn:hover { color: var(--text); border-color: #aaa; }

  /* ── Tab bar (mobile / tablet only) ── */
  .tab-bar {
    display: none;
    overflow-x: auto;
    flex-shrink: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding: 0 12px;
    gap: 0;
    scrollbar-width: none;
  }
  .tab-bar::-webkit-scrollbar { display: none; }

  .tab-btn {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding: 9px 12px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    cursor: pointer;
    white-space: nowrap;
    transition: color 0.15s, border-color 0.15s;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .tab-btn .tab-count {
    font-size: 11px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 0 5px;
    line-height: 16px;
  }
  .tab-btn.active {
    color: var(--text);
    border-bottom-color: var(--tab-accent, #111);
  }
  .tab-btn.active .tab-count {
    background: var(--tab-accent, #111);
    border-color: var(--tab-accent, #111);
    color: #fff;
  }

  /* ── Board ── */
  .board {
    display: flex;
    flex: 1;
    overflow-x: auto;
    overflow-y: hidden;
    padding: 14px;
    gap: 12px;
  }

  /* ── Column ── */
  .column {
    flex: 0 0 220px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    /* min-height:0 lets the inner .cards honor overflow-y:auto.
       Without it, a flex child's default min-height is auto, so the
       column grows to the content size and breaks vertical scrolling. */
    min-height: 0;
  }

  .col-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 2px 8px;
    border-bottom: 2px solid var(--col-accent);
    flex-shrink: 0;
  }

  .col-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--col-accent);
    flex-shrink: 0;
  }

  .col-name {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .col-count {
    font-size: 11px;
    color: var(--muted);
    margin-left: auto;
  }

  /* ── Cards ── */
  .cards {
    flex: 1;
    /* min-height:0 disables the flex-item default min-height:auto, so
       overflow-y:auto actually constrains the cards stack to its parent
       column height instead of letting it grow indefinitely. */
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 2px;
  }
  .cards::-webkit-scrollbar { width: 3px; }
  .cards::-webkit-scrollbar-track { background: transparent; }
  .cards::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }

  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 11px;
    cursor: pointer;
    transition: box-shadow 0.12s, border-color 0.12s;
  }
  .card:hover {
    box-shadow: 0 2px 8px rgba(0,0,0,0.07);
    border-color: #ccc;
  }
  .card.dragging {
    opacity: 0.4;
  }
  .cards.drag-over {
    background: rgba(59,130,246,0.06);
    border-radius: 6px;
  }

  .card-title {
    font-size: 13px;
    font-weight: 500;
    line-height: 1.45;
    margin-bottom: 7px;
  }

  .card-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }

  .priority-badge {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--p-color);
    padding: 1px 5px;
    border-radius: 3px;
    border: 1px solid var(--p-color);
    opacity: 0.85;
  }

  .tag {
    font-size: 10px;
    color: var(--muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 5px;
  }

  .due-date {
    font-size: 10px;
    color: var(--muted);
    margin-left: auto;
  }
  .due-date.overdue { color: var(--priority-critical); }
  .due-date.soon    { color: var(--priority-high); }

  /* ── Modal ── */
  .modal-overlay {
    display: none;
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.25);
    z-index: 100;
    align-items: center;
    justify-content: center;
  }
  .modal-overlay.open { display: flex; }

  .modal {
    background: var(--surface);
    border-radius: 10px;
    border: 1px solid var(--border);
    box-shadow: 0 8px 32px rgba(0,0,0,0.12);
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .modal-header {
    padding: 16px 18px 12px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .modal-priority-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-top: 4px;
  }

  .modal-title {
    font-size: 15px;
    font-weight: 600;
    line-height: 1.4;
    flex: 1;
  }

  .modal-close {
    background: none;
    border: none;
    font-size: 18px;
    color: var(--muted);
    cursor: pointer;
    line-height: 1;
    padding: 0 2px;
  }
  .modal-close:hover { color: var(--text); }

  .modal-body {
    padding: 14px 18px;
    overflow-y: auto;
    flex: 1;
  }

  .modal-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
    margin-bottom: 14px;
  }

  .meta-item { font-size: 11px; color: var(--muted); }
  .meta-item strong { color: var(--text); font-weight: 500; }

  .modal-content {
    font-size: 12.5px;
    line-height: 1.7;
    font-family: inherit;
  }
  .modal-content h1, .modal-content h2, .modal-content h3 {
    margin: 14px 0 6px;
    line-height: 1.3;
  }
  .modal-content h1 { font-size: 16px; }
  .modal-content h2 { font-size: 14px; }
  .modal-content h3 { font-size: 13px; }
  .modal-content p { margin: 0 0 8px; }
  .modal-content ul, .modal-content ol {
    margin: 0 0 8px;
    padding-left: 20px;
  }
  .modal-content li { margin: 2px 0; }
  .modal-content code {
    font-family: "SF Mono", "Menlo", "Consolas", monospace;
    font-size: 11.5px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 4px;
  }
  .modal-content pre {
    background: #1e1e1e;
    color: #d4d4d4;
    border-radius: 5px;
    padding: 10px 12px;
    margin: 8px 0;
    overflow-x: auto;
    font-family: "SF Mono", "Menlo", "Consolas", monospace;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .modal-content pre code {
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    font-size: inherit;
  }
  .modal-content blockquote {
    border-left: 3px solid var(--border);
    margin: 8px 0;
    padding: 4px 12px;
    color: var(--muted);
  }
  .modal-content hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 12px 0;
  }
  .modal-content a {
    color: #3b82f6;
    text-decoration: none;
  }
  .modal-content a:hover { text-decoration: underline; }
  .modal-content strong { font-weight: 600; }
  .modal-content em { font-style: italic; }

  .empty {
    font-size: 12px;
    color: var(--muted);
    text-align: center;
    padding: 20px 0;
  }

  .loading {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    color: var(--muted);
    background: var(--bg);
    z-index: 200;
  }

  /* ── Chat Panel ── */
  .chat-toggle {
    position: fixed;
    bottom: 16px;
    right: 16px;
    z-index: 50;
    background: #111;
    color: #fff;
    border: none;
    border-radius: 20px;
    padding: 8px 16px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: 0 2px 12px rgba(0,0,0,0.15);
    transition: background 0.15s, transform 0.15s;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .chat-toggle:hover { background: #333; transform: translateY(-1px); }
  .chat-toggle.has-panel { display: none; }

  .chat-panel {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    height: 340px;
    z-index: 60;
    background: var(--surface);
    border-top: 1px solid var(--border);
    box-shadow: 0 -4px 20px rgba(0,0,0,0.08);
    display: none;
    flex-direction: column;
  }
  .chat-panel.open { display: flex; }

  .chat-resize-handle {
    height: 12px;
    cursor: ns-resize;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    touch-action: none;
  }
  .chat-resize-handle::after {
    content: '';
    width: 36px;
    height: 3px;
    border-radius: 2px;
    background: var(--border);
    transition: background 0.15s;
  }
  .chat-resize-handle:hover::after,
  .chat-resize-handle:active::after { background: #999; }

  .chat-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .chat-title {
    font-size: 12px;
    font-weight: 600;
  }

  .chat-model-select {
    font-size: 10px;
    color: var(--muted);
    padding: 2px 6px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    outline: none;
  }
  .chat-model-select:focus { border-color: #999; }
  .chat-model-select:disabled { cursor: default; opacity: 0.7; }

  .chat-effort-select {
    font-size: 10px;
    color: var(--muted);
    padding: 2px 6px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    outline: none;
  }
  .chat-effort-select:focus { border-color: #999; }

  .chat-header-btn {
    background: none;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 11px;
    color: var(--muted);
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .chat-header-btn:hover { color: var(--text); border-color: #aaa; }

  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .chat-messages::-webkit-scrollbar { width: 3px; }
  .chat-messages::-webkit-scrollbar-track { background: transparent; }
  .chat-messages::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }

  .chat-msg table,
  .modal-content table {
    border-collapse: collapse;
    margin: 6px 0;
    font-size: 0.95em;
    max-width: 100%;
  }
  .chat-msg th,
  .chat-msg td,
  .modal-content th,
  .modal-content td {
    padding: 4px 8px;
    border: 1px solid var(--border);
    text-align: left;
    white-space: normal;
    vertical-align: top;
  }
  .chat-msg th,
  .modal-content th {
    background: var(--surface);
    font-weight: 600;
  }

  .chat-msg {
    max-width: 85%;
    padding: 7px 11px;
    border-radius: 8px;
    font-size: 12.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .chat-msg.user {
    align-self: flex-end;
    background: #e8f0fe;
    color: #1a3a5c;
    border-bottom-right-radius: 2px;
  }

  .chat-msg.assistant {
    align-self: flex-start;
    background: var(--bg);
    border: 1px solid var(--border);
    border-bottom-left-radius: 2px;
    white-space: normal;
  }
  .chat-msg.assistant h1, .chat-msg.assistant h2, .chat-msg.assistant h3 {
    margin: 8px 0 4px; line-height: 1.3;
  }
  .chat-msg.assistant h1 { font-size: 15px; }
  .chat-msg.assistant h2 { font-size: 13.5px; }
  .chat-msg.assistant h3 { font-size: 12.5px; }
  .chat-msg.assistant p { margin: 0 0 6px; }
  .chat-msg.assistant p:last-child { margin-bottom: 0; }
  .chat-msg.assistant ul, .chat-msg.assistant ol {
    margin: 0 0 6px; padding-left: 18px;
  }
  .chat-msg.assistant li { margin: 1px 0; }
  .chat-msg.assistant code {
    font-family: "SF Mono","Menlo","Consolas",monospace;
    font-size: 11px; background: var(--surface);
    border: 1px solid var(--border); border-radius: 3px; padding: 1px 4px;
  }
  .chat-msg.assistant pre {
    background: #1e1e1e; color: #d4d4d4; border-radius: 5px;
    padding: 8px 10px; margin: 6px 0; overflow-x: auto;
    font-family: "SF Mono","Menlo","Consolas",monospace;
    font-size: 11px; line-height: 1.45;
  }
  .chat-msg.assistant pre code {
    background: none; border: none; padding: 0; color: inherit; font-size: inherit;
  }
  .chat-msg.assistant blockquote {
    border-left: 3px solid var(--border); margin: 6px 0; padding: 2px 10px; color: var(--muted);
  }
  .chat-msg.assistant strong { font-weight: 600; }
  .chat-msg.assistant em { font-style: italic; }

  .chat-msg.system {
    align-self: center;
    background: transparent;
    color: var(--muted);
    font-size: 11px;
    padding: 2px 0;
  }

  .chat-msg.tool {
    align-self: flex-start;
    background: transparent;
    color: var(--muted);
    font-size: 11px;
    padding: 3px 8px;
    border-left: 2px solid #22c55e;
    border-radius: 0;
    max-width: 95%;
  }
  .chat-msg.tool.error { border-left-color: var(--priority-critical); }

  .chat-typing {
    align-self: flex-start;
    font-size: 11px;
    color: var(--muted);
    padding: 4px 0;
  }
  .chat-typing span {
    animation: blink 1.2s infinite;
    display: inline-block;
  }
  .chat-typing span:nth-child(2) { animation-delay: 0.2s; }
  .chat-typing span:nth-child(3) { animation-delay: 0.4s; }
  @keyframes blink {
    0%, 60%, 100% { opacity: 0.2; }
    30% { opacity: 1; }
  }

  .chat-input-area {
    display: flex;
    padding: 8px 14px;
    gap: 8px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }

  .chat-input-area input {
    flex: 1;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    font-size: 12.5px;
    font-family: inherit;
    outline: none;
    transition: border-color 0.15s;
  }
  .chat-input-area input:focus { border-color: #999; }
  .chat-input-area input:disabled { background: var(--bg); color: var(--muted); }

  .chat-send-btn {
    background: #111;
    color: #fff;
    border: none;
    border-radius: 6px;
    padding: 7px 14px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s;
    white-space: nowrap;
  }
  .chat-send-btn:hover { background: #333; }
  .chat-send-btn:disabled { background: #999; cursor: not-allowed; }

  /* ── Responsive ── */

  /* Tablet: 600-899px — tab switching, single column visible */
  @media (max-width: 899px) {
    .tab-bar  { display: flex; }
    .board    { overflow-x: hidden; padding: 12px; }
    .column   { flex: 0 0 100%; display: none; }
    .column.active { display: flex; }
  }

  /* Mobile: <600px */
  @media (max-width: 599px) {
    header { padding: 10px 14px; }
    header h1 { font-size: 14px; }
    .subtitle { display: none; }
    .board { padding: 10px; }
    .card-title { font-size: 13px; }
    .modal { border-radius: 12px 12px 0 0; width: 100%; max-height: 85vh; position: fixed; bottom: 0; }
    .modal-overlay { align-items: flex-end; }
    .chat-panel { height: 60vh; }
    .chat-toggle { bottom: 12px; right: 12px; }
  }
</style>
</head>
<body>

<div class="loading" id="loading">Loading…</div>

<header>
  <h1>Icebox</h1>
  <span class="subtitle" id="subtitle"></span>
  <div class="spacer"></div>
  <select class="swimlane-select" id="swimlane-select" onchange="switchSwimlane(this.value)">
    <option value="">All</option>
  </select>
  <button class="refresh-btn" onclick="loadTasks()">Refresh</button>
</header>

<div class="tab-bar" id="tab-bar"></div>

<div class="board" id="board"></div>

<div class="modal-overlay" id="modal" onclick="closeModal(event)">
  <div class="modal" id="modal-inner"></div>
</div>

<!-- Chat toggle button -->
<button class="chat-toggle" id="chat-toggle" onclick="toggleChat()">AI Chat</button>

<!-- Chat panel -->
<div class="chat-panel" id="chat-panel">
  <div class="chat-resize-handle" id="chat-resize-handle"></div>
  <div class="chat-header">
    <span class="chat-title">AI Chat</span>
    <select class="chat-model-select" id="chat-model-select" onchange="switchModel(this.value)" disabled>
      <option value="">disconnected</option>
    </select>
    <select class="chat-effort-select" id="chat-effort-select" onchange="switchEffort(this.value)">
      <option value="low">Low</option>
      <option value="medium">Medium</option>
      <option value="high" selected>High</option>
      <option value="max">Max</option>
    </select>
    <div class="spacer"></div>
    <button class="chat-header-btn" onclick="clearChat()">Clear</button>
    <button class="chat-header-btn" onclick="toggleChat()">—</button>
  </div>
  <div class="chat-messages" id="chat-messages"></div>
  <div class="chat-input-area">
    <input type="text" id="chat-input" placeholder="Ask AI..."
           onkeydown="if(event.key==='Enter')sendChat()" disabled />
    <button class="chat-send-btn" id="chat-send" onclick="sendChat()" disabled>Send</button>
  </div>
</div>

<script>
const COLUMNS = [
  { key: 'icebox',     label: 'Icebox',      accent: '#94a3b8' },
  { key: 'emergency',  label: 'Emergency',   accent: '#ef4444' },
  { key: 'inprogress', label: 'In Progress', accent: '#3b82f6' },
  { key: 'testing',    label: 'Testing',     accent: '#f59e0b' },
  { key: 'complete',   label: 'Complete',    accent: '#22c55e' },
];

const PRIORITY = {
  low:      { label: 'Low',      color: '#94a3b8' },
  medium:   { label: 'Medium',   color: '#3b82f6' },
  high:     { label: 'High',     color: '#f59e0b' },
  critical: { label: 'Critical', color: '#ef4444' },
};

let allTasks = [];
let activeTab = COLUMNS[0].key;
let activeSwimlane = '';

async function loadTasks() {
  try {
    const res = await fetch('/api/tasks');
    allTasks = await res.json();
    render(allTasks);
    document.getElementById('loading').style.display = 'none';
    const shown = activeSwimlane
      ? allTasks.filter(t => (t.swimlane || '') === activeSwimlane).length
      : allTasks.length;
    const suffix = activeSwimlane ? ` in @${activeSwimlane}` : '';
    document.getElementById('subtitle').textContent =
      `${shown} task${shown !== 1 ? 's' : ''}${suffix}`;
  } catch (e) {
    document.getElementById('loading').textContent = 'Failed to load tasks.';
  }
}

function render(tasks) {
  updateSwimlaneSelect(tasks);
  const filtered = activeSwimlane
    ? tasks.filter(t => (t.swimlane || '') === activeSwimlane)
    : tasks;
  renderBoard(filtered);
  renderTabs(filtered);
  applyActiveTab();
}

function updateSwimlaneSelect(tasks) {
  const lanes = [...new Set(tasks.map(t => t.swimlane || '').filter(Boolean))].sort();
  const sel = document.getElementById('swimlane-select');
  const prev = sel.value;
  sel.innerHTML = '<option value="">All</option>';
  for (const lane of lanes) {
    const count = tasks.filter(t => (t.swimlane || '') === lane).length;
    const opt = document.createElement('option');
    opt.value = lane;
    opt.textContent = `@${lane} (${count})`;
    sel.appendChild(opt);
  }
  sel.value = prev;
}

function switchSwimlane(value) {
  activeSwimlane = value;
  render(allTasks);
}

function renderBoard(tasks) {
  const board = document.getElementById('board');
  board.innerHTML = '';
  for (const col of COLUMNS) {
    const colTasks = tasks.filter(t => t.column === col.key);
    const el = document.createElement('div');
    el.className = 'column';
    el.dataset.col = col.key;
    el.innerHTML = `
      <div class="col-header" style="--col-accent:${col.accent}">
        <div class="col-dot"></div>
        <span class="col-name">${col.label}</span>
        <span class="col-count">${colTasks.length}</span>
      </div>
      <div class="cards" id="col-${col.key}"></div>
    `;
    board.appendChild(el);

    const container = el.querySelector(`#col-${col.key}`);
    // Drop target
    container.addEventListener('dragover', e => {
      e.preventDefault();
      container.classList.add('drag-over');
    });
    container.addEventListener('dragleave', () => {
      container.classList.remove('drag-over');
    });
    container.addEventListener('drop', e => {
      e.preventDefault();
      container.classList.remove('drag-over');
      const taskId = e.dataTransfer.getData('text/plain');
      if (taskId && col.key) moveTask(taskId, col.key);
    });

    if (colTasks.length === 0) {
      container.innerHTML = '<div class="empty">—</div>';
    } else {
      for (const task of colTasks) container.appendChild(makeCard(task));
    }
  }
}

function renderTabs(tasks) {
  const bar = document.getElementById('tab-bar');
  bar.innerHTML = '';
  for (const col of COLUMNS) {
    const count = tasks.filter(t => t.column === col.key).length;
    const btn = document.createElement('button');
    btn.className = 'tab-btn';
    btn.dataset.col = col.key;
    btn.style.setProperty('--tab-accent', col.accent);
    btn.innerHTML = `${col.label}<span class="tab-count">${count}</span>`;
    btn.onclick = () => switchTab(col.key);
    bar.appendChild(btn);
  }
}

function switchTab(key) {
  activeTab = key;
  applyActiveTab();
  // scroll active tab button into view
  const btn = document.querySelector(`.tab-btn[data-col="${key}"]`);
  if (btn) btn.scrollIntoView({ inline: 'nearest', behavior: 'smooth' });
}

function applyActiveTab() {
  document.querySelectorAll('.column').forEach(el => {
    el.classList.toggle('active', el.dataset.col === activeTab);
  });
  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.col === activeTab);
  });
}

function makeCard(task) {
  const p = PRIORITY[task.priority] || PRIORITY.medium;
  const el = document.createElement('div');
  el.className = 'card';
  el.style.setProperty('--p-color', p.color);
  el.draggable = true;
  el.dataset.taskId = task.id;

  el.addEventListener('dragstart', e => {
    e.dataTransfer.setData('text/plain', task.id);
    e.dataTransfer.effectAllowed = 'move';
    el.classList.add('dragging');
  });
  el.addEventListener('dragend', () => {
    el.classList.remove('dragging');
  });

  const tags = (task.tags || []).slice(0, 3)
    .map(t => `<span class="tag">${esc(t)}</span>`).join('');
  const due = task.due_date ? formatDue(task.due_date) : '';

  el.innerHTML = `
    <div class="card-title">${esc(task.title)}</div>
    <div class="card-meta">
      <span class="priority-badge">${p.label}</span>
      ${tags}${due}
    </div>
  `;
  el.onclick = () => openModal(task);
  return el;
}

function formatDue(iso) {
  const days = Math.ceil((new Date(iso) - Date.now()) / 86400000);
  let cls = 'due-date';
  if (days < 0) cls += ' overdue';
  else if (days <= 3) cls += ' soon';
  const label = days < 0 ? `${-days}d overdue` : days === 0 ? 'today' : `${days}d left`;
  return `<span class="${cls}">${label}</span>`;
}

function openModal(task) {
  const p = PRIORITY[task.priority] || PRIORITY.medium;
  const col = COLUMNS.find(c => c.key === task.column) || COLUMNS[0];
  const tags = (task.tags || []).map(t => `<span class="tag">${esc(t)}</span>`).join(' ');
  const body = task.body && task.body.trim()
    ? `<div class="modal-content">${renderMarkdown(task.body.trim())}</div>` : '';
  const created = task.created_at ? new Date(task.created_at).toLocaleDateString() : '—';
  const due     = task.due_date   ? new Date(task.due_date).toLocaleDateString()   : '—';
  const start   = task.start_date ? new Date(task.start_date).toLocaleDateString() : '—';

  document.getElementById('modal-inner').innerHTML = `
    <div class="modal-header">
      <div class="modal-priority-dot" style="background:${p.color}"></div>
      <div class="modal-title">${esc(task.title)}</div>
      <button class="modal-close" onclick="closeModal()">×</button>
    </div>
    <div class="modal-body">
      <div class="modal-meta">
        <span class="meta-item"><strong>Column</strong> ${col.label}</span>
        <span class="meta-item"><strong>Priority</strong> ${p.label}</span>
        ${task.swimlane ? `<span class="meta-item"><strong>Swimlane</strong> ${esc(task.swimlane)}</span>` : ''}
        <span class="meta-item"><strong>Created</strong> ${created}</span>
        ${start !== '—' ? `<span class="meta-item"><strong>Start</strong> ${start}</span>` : ''}
        ${due   !== '—' ? `<span class="meta-item"><strong>Due</strong> ${due}</span>`     : ''}
        ${task.progress ? `<span class="meta-item"><strong>Progress</strong> ${task.progress.done}/${task.progress.total}</span>` : ''}
      </div>
      ${tags ? `<div style="margin-bottom:12px;display:flex;gap:5px;flex-wrap:wrap">${tags}</div>` : ''}
      ${body}
    </div>
  `;
  document.getElementById('modal').classList.add('open');
}

function closeModal(e) {
  if (!e || e.target === document.getElementById('modal'))
    document.getElementById('modal').classList.remove('open');
}

document.addEventListener('keydown', e => { if (e.key === 'Escape') closeModal(); });

function esc(s) {
  return String(s)
    .replace(/&/g,'&amp;').replace(/</g,'&lt;')
    .replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function renderMarkdown(src) {
  const lines = src.split('\n');
  let html = '';
  let inCode = false, codeLang = '', codeLines = [];
  let inList = false, listType = '';

  function closeList() {
    if (inList) { html += listType === 'ul' ? '</ul>' : '</ol>'; inList = false; }
  }

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Fenced code block
    if (line.trimStart().startsWith('```')) {
      if (!inCode) {
        closeList();
        codeLang = line.trim().slice(3).trim();
        inCode = true; codeLines = [];
      } else {
        html += '<pre><code>' + esc(codeLines.join('\n')) + '</code></pre>';
        inCode = false;
      }
      continue;
    }
    if (inCode) { codeLines.push(line); continue; }

    // GFM table: header row + `| --- | --- |` separator + data rows
    if (line.includes('|') && i + 1 < lines.length) {
      const sep = lines[i + 1];
      if (/^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(sep)) {
        closeList();
        const parseCells = (ln) => {
          let s = ln.trim();
          if (s.startsWith('|')) s = s.slice(1);
          if (s.endsWith('|')) s = s.slice(0, -1);
          return s.split('|').map(c => c.trim());
        };
        const aligns = parseCells(sep).map(cell => {
          const l = cell.startsWith(':'), r = cell.endsWith(':');
          if (l && r) return 'center';
          if (r) return 'right';
          if (l) return 'left';
          return '';
        });
        const headers = parseCells(line);
        html += '<table><thead><tr>';
        headers.forEach((h, idx) => {
          const a = aligns[idx] ? ` style="text-align:${aligns[idx]}"` : '';
          html += `<th${a}>${inline(h)}</th>`;
        });
        html += '</tr></thead><tbody>';
        i += 2;
        while (i < lines.length && lines[i].includes('|') && lines[i].trim() !== '') {
          const cells = parseCells(lines[i]);
          html += '<tr>';
          cells.forEach((c, idx) => {
            const a = aligns[idx] ? ` style="text-align:${aligns[idx]}"` : '';
            html += `<td${a}>${inline(c)}</td>`;
          });
          html += '</tr>';
          i++;
        }
        html += '</tbody></table>';
        i--;
        continue;
      }
    }

    // Headings
    const hm = line.match(/^(#{1,3})\s+(.+)/);
    if (hm) { closeList(); const lv = hm[1].length; html += `<h${lv}>${inline(hm[2])}</h${lv}>`; continue; }

    // Horizontal rule
    if (/^(-{3,}|\*{3,}|_{3,})\s*$/.test(line)) { closeList(); html += '<hr>'; continue; }

    // Blockquote
    if (line.startsWith('> ')) { closeList(); html += `<blockquote>${inline(line.slice(2))}</blockquote>`; continue; }

    // Unordered list
    const ulm = line.match(/^(\s*)[-*+]\s+(.+)/);
    if (ulm) {
      if (!inList || listType !== 'ul') { closeList(); html += '<ul>'; inList = true; listType = 'ul'; }
      html += `<li>${inline(ulm[2])}</li>`;
      continue;
    }

    // Ordered list
    const olm = line.match(/^(\s*)\d+\.\s+(.+)/);
    if (olm) {
      if (!inList || listType !== 'ol') { closeList(); html += '<ol>'; inList = true; listType = 'ol'; }
      html += `<li>${inline(olm[2])}</li>`;
      continue;
    }

    closeList();

    // Empty line
    if (line.trim() === '') continue;

    // Paragraph
    html += `<p>${inline(line)}</p>`;
  }

  if (inCode) html += '<pre><code>' + esc(codeLines.join('\n')) + '</code></pre>';
  closeList();
  return html;
}

function inline(text) {
  let s = esc(text);
  // Images (before links)
  s = s.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, '<img src="$2" alt="$1" style="max-width:100%">');
  // Links
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  // Bold
  s = s.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/__(.+?)__/g, '<strong>$1</strong>');
  // Italic
  s = s.replace(/\*(.+?)\*/g, '<em>$1</em>');
  s = s.replace(/_(.+?)_/g, '<em>$1</em>');
  // Strikethrough
  s = s.replace(/~~(.+?)~~/g, '<del>$1</del>');
  // Inline code
  s = s.replace(/`([^`]+)`/g, '<code>$1</code>');
  return s;
}

// ── Chat ──

let ws = null;
let chatOpen = false;
let aiBusy = false;
let currentAssistantEl = null;
let currentAssistantRaw = '';
let reconnectTimer = null;

// ── Chat resize ──
(function() {
  const handle = document.getElementById('chat-resize-handle');
  const panel = document.getElementById('chat-panel');
  let startY = 0, startH = 0, dragging = false;

  function begin(y) {
    startY = y;
    startH = panel.offsetHeight;
    dragging = true;
    document.body.style.cursor = 'ns-resize';
    document.body.style.userSelect = 'none';
  }

  function move(y) {
    if (!dragging) return;
    const h = Math.min(Math.max(startH + (startY - y), 150), window.innerHeight - 60);
    panel.style.height = h + 'px';
  }

  function end() {
    if (!dragging) return;
    dragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  }

  handle.addEventListener('mousedown', e => { e.preventDefault(); begin(e.clientY); });
  document.addEventListener('mousemove', e => move(e.clientY));
  document.addEventListener('mouseup', end);

  handle.addEventListener('touchstart', e => { e.preventDefault(); begin(e.touches[0].clientY); }, { passive: false });
  document.addEventListener('touchmove', e => { if (dragging) { e.preventDefault(); move(e.touches[0].clientY); } }, { passive: false });
  document.addEventListener('touchend', end);
})();

function toggleChat() {
  chatOpen = !chatOpen;
  const panel = document.getElementById('chat-panel');
  const toggle = document.getElementById('chat-toggle');
  panel.classList.toggle('open', chatOpen);
  toggle.classList.toggle('has-panel', chatOpen);

  if (chatOpen) {
    if (!ws || ws.readyState !== WebSocket.OPEN) connectChat();
    document.getElementById('chat-input').focus();
  }
}

function connectChat() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }

  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  ws = new WebSocket(`${proto}//${location.host}/ws/chat`);

  ws.onopen = async () => {
    await loadChatModels();
    updateChatStatus('connected');
    document.getElementById('chat-input').disabled = false;
    document.getElementById('chat-send').disabled = false;
    document.getElementById('chat-model-select').disabled = false;
  };

  ws.onclose = () => {
    updateChatStatus('disconnected');
    document.getElementById('chat-input').disabled = true;
    document.getElementById('chat-send').disabled = true;
    document.getElementById('chat-model-select').disabled = true;
    ws = null;
    if (chatOpen) reconnectTimer = setTimeout(connectChat, 3000);
  };

  ws.onerror = () => { updateChatStatus('error'); };

  ws.onmessage = (e) => {
    try { handleWsMessage(JSON.parse(e.data)); } catch (_) { /* ignore parse errors */ }
  };
}

function handleWsMessage(msg) {
  switch (msg.type) {
    case 'text_delta':
      if (!currentAssistantEl) {
        removeTypingIndicator();
        currentAssistantRaw = '';
        currentAssistantEl = appendMsg('assistant', '');
      }
      currentAssistantRaw += msg.text;
      currentAssistantEl.innerHTML = renderMarkdown(currentAssistantRaw);
      scrollChat();
      break;

    case 'tool_start':
      removeTypingIndicator();
      appendMsg('tool', `> ${msg.name}(${truncText(msg.input, 80)})`);
      showTypingIndicator();
      break;

    case 'tool_end': {
      removeTypingIndicator();
      const cls = msg.is_error ? 'tool error' : 'tool';
      const prefix = msg.is_error ? 'ERR' : 'OK';
      appendMsg(cls, `< ${msg.name}: ${prefix} ${truncText(msg.output, 120)}`);
      showTypingIndicator();
      break;
    }

    case 'turn_complete':
      currentAssistantEl = null;
      currentAssistantRaw = '';
      aiBusy = false;
      removeTypingIndicator();
      updateInputState();
      loadTasks(); // refresh board — AI may have changed tasks
      break;

    case 'error':
      removeTypingIndicator();
      currentAssistantEl = null;
      currentAssistantRaw = '';
      aiBusy = false;
      appendMsg('system', msg.message);
      updateInputState();
      break;

    case 'session_cleared':
      document.getElementById('chat-messages').innerHTML = '';
      appendMsg('system', 'Session cleared.');
      break;

    case 'status':
      if (msg.ai_available) {
        updateChatStatus(msg.model || 'ready');
      } else {
        updateChatStatus('no API key');
      }
      break;

    case 'usage':
      // silently ignore usage messages
      break;
  }
}

function sendChat() {
  const inp = document.getElementById('chat-input');
  const text = inp.value.trim();
  if (!text || aiBusy || !ws || ws.readyState !== WebSocket.OPEN) return;

  appendMsg('user', text);
  ws.send(JSON.stringify({ type: 'send_message', text: text }));
  inp.value = '';
  aiBusy = true;
  updateInputState();
  showTypingIndicator();
}

function clearChat() {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'clear_session' }));
  }
}

function appendMsg(cls, text) {
  const msgs = document.getElementById('chat-messages');
  const el = document.createElement('div');
  el.className = 'chat-msg ' + cls;
  el.textContent = text;
  msgs.appendChild(el);
  scrollChat();
  return el;
}

function scrollChat() {
  const msgs = document.getElementById('chat-messages');
  msgs.scrollTop = msgs.scrollHeight;
}

function showTypingIndicator() {
  removeTypingIndicator();
  const msgs = document.getElementById('chat-messages');
  const el = document.createElement('div');
  el.className = 'chat-typing';
  el.id = 'chat-typing';
  el.innerHTML = '<span>.</span><span>.</span><span>.</span>';
  msgs.appendChild(el);
  scrollChat();
}

function removeTypingIndicator() {
  const el = document.getElementById('chat-typing');
  if (el) el.remove();
}

function updateInputState() {
  const inp = document.getElementById('chat-input');
  const btn = document.getElementById('chat-send');
  const connected = ws && ws.readyState === WebSocket.OPEN;
  inp.disabled = !connected || aiBusy;
  btn.disabled = !connected || aiBusy;
  inp.placeholder = aiBusy ? 'AI is thinking...' : 'Ask AI...';
}

let chatModels = [];

async function loadChatModels() {
  try {
    const res = await fetch('/api/models');
    chatModels = await res.json();
  } catch (e) { chatModels = []; }
}

function updateChatStatus(text) {
  const sel = document.getElementById('chat-model-select');
  if (chatModels.length > 0) {
    // Populate with model options if not yet done
    if (sel.options.length <= 1 || sel.options[0].value === '') {
      sel.innerHTML = '';
      for (const m of chatModels) {
        const opt = document.createElement('option');
        opt.value = m.alias;
        opt.textContent = m.display_name;
        sel.appendChild(opt);
      }
    }
    // Select matching model
    const match = chatModels.find(m => text.includes(m.id) || text === m.alias);
    if (match) sel.value = match.alias;
  } else {
    sel.innerHTML = `<option value="">${text}</option>`;
  }
}

function switchModel(alias) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'switch_model', model: alias }));
  }
}

let currentEffort = 'high';

function switchEffort(value) {
  currentEffort = value;
}

function truncText(s, max) {
  if (!s) return '';
  return s.length > max ? s.slice(0, max) + '...' : s;
}

async function moveTask(taskId, column) {
  try {
    const res = await fetch('/api/tasks/move', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ task_id: taskId, column: column })
    });
    if (res.ok) loadTasks();
  } catch (e) { /* ignore */ }
}

// Auto-refresh: poll every 3s while the tab is visible. Skip when a modal
// is open or the AI is mid-stream so the user's interaction isn't disrupted.
const REFRESH_INTERVAL_MS = 3000;
setInterval(() => {
  if (document.hidden) return;
  if (document.querySelector('.modal-overlay.open')) return;
  if (aiBusy) return;
  loadTasks();
}, REFRESH_INTERVAL_MS);

// Refresh immediately when the tab regains focus after being hidden.
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) loadTasks();
});

loadTasks();
</script>
</body>
</html>
"#;
