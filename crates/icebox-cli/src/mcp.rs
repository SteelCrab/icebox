//! MCP (Model Context Protocol) server over stdio.
//!
//! Implements JSON-RPC 2.0 protocol for Claude Code integration.
//! Exposes all icebox tools as MCP tools.

use anyhow::Result;
use icebox_runtime::ToolExecutor;
use icebox_tools::IceboxToolExecutor;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

// ── JSON-RPC 2.0 types ──

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ── JSON-RPC error codes ──

const PARSE_ERROR: i64 = -32700;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

// ── MCP protocol version ──

const PROTOCOL_VERSION: &str = "2024-11-05";

// ── Public entry point ──

/// Run the MCP server, reading JSON-RPC messages from stdin and writing responses to stdout.
///
/// # Errors
/// Returns an error if workspace initialization fails.
pub fn run_mcp_server(workspace: &Path) -> Result<()> {
    let store = icebox_task::store::TaskStore::open(workspace)?;
    let store = Arc::new(Mutex::new(store));
    let tools = IceboxToolExecutor::new(workspace.to_path_buf(), store)?;

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let reader = stdin.lock();
    let mut writer = stdout.lock();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // EOF or read error
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(req) => handle_request(&tools, req),
            Err(e) => Some(error_response(
                Value::Null,
                PARSE_ERROR,
                &format!("Parse error: {e}"),
            )),
        };

        if let Some(resp) = response {
            let json = serde_json::to_string(&resp).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"serialization failed"}}"#.to_string()
            });
            let _ = writeln!(writer, "{json}");
            let _ = writer.flush();
        }
    }

    Ok(())
}

// ── Request dispatch ──

fn handle_request(tools: &IceboxToolExecutor, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    // Notifications have no id — no response expected
    let id = req.id?;

    match req.method.as_str() {
        "initialize" => Some(handle_initialize(id)),
        "tools/list" => Some(handle_tools_list(id, tools)),
        "tools/call" => Some(handle_tools_call(id, tools, req.params)),
        "ping" => Some(success_response(id, json!({}))),
        _ => Some(error_response(
            id,
            METHOD_NOT_FOUND,
            &format!("Method not found: {}", req.method),
        )),
    }
}

// ── initialize ──

fn handle_initialize(id: Value) -> JsonRpcResponse {
    let version = env!("CARGO_PKG_VERSION");

    success_response(
        id,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "icebox",
                "version": version
            },
            "instructions": concat!(
                "Icebox is a kanban board for project task management. ",
                "Use list_tasks to see the board state. ",
                "Use create_task to add new tasks, update_task to modify, move_task to change columns. ",
                "Use save_memory/list_memories/delete_memory for persistent context. ",
                "When the user discusses tasks, features, or bugs, proactively suggest creating or updating tasks on the board."
            )
        }),
    )
}

// ── tools/list ──

fn handle_tools_list(id: Value, tools: &IceboxToolExecutor) -> JsonRpcResponse {
    let definitions = tools.tool_definitions();

    let mcp_tools: Vec<Value> = definitions
        .iter()
        .map(|def| {
            json!({
                "name": def.name,
                "description": def.description.as_deref().unwrap_or(""),
                "inputSchema": def.input_schema
            })
        })
        .collect();

    success_response(id, json!({ "tools": mcp_tools }))
}

// ── tools/call ──

fn handle_tools_call(
    id: Value,
    tools: &IceboxToolExecutor,
    params: Option<Value>,
) -> JsonRpcResponse {
    let Some(params) = params else {
        return error_response(id, INVALID_PARAMS, "Missing params");
    };

    let name = match params.get("name").and_then(Value::as_str) {
        Some(n) => n.to_string(),
        None => return error_response(id, INVALID_PARAMS, "Missing 'name' in params"),
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let input = match serde_json::to_string(&arguments) {
        Ok(s) => s,
        Err(e) => {
            return error_response(
                id,
                INVALID_PARAMS,
                &format!("Failed to serialize arguments: {e}"),
            )
        }
    };

    match tools.execute(&name, &input) {
        Ok(output) => success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": output }],
                "isError": false
            }),
        ),
        Err(e) => success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": format!("Error: {e}") }],
                "isError": true
            }),
        ),
    }
}

// ── Response helpers ──

fn success_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn error_response(id: Value, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    }
}
