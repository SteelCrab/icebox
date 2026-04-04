//! OAuth request/response transforms — ported from `opencode-anthropic-auth`.
//!
//! Mirrors `src/transform.ts` and `src/constants.ts` from:
//! <https://github.com/ex-machina-co/opencode-anthropic-auth>

use reqwest::header::{HeaderMap, HeaderValue};
use sha2::{Digest, Sha256};

use crate::error::ApiError;
use crate::types::{InputContentBlock, MessageRequest};

// ── Constants (constants.ts) ──

pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const TOOL_PREFIX: &str = "mcp_";

pub const REQUIRED_BETAS: &[&str] = &[
    "claude-code-20250219",
    "oauth-2025-04-20",
    "interleaved-thinking-2025-05-14",
    "prompt-caching-scope-2026-01-05",
    "context-management-2025-06-27",
];

const DEFAULT_CLAUDE_CLI_VERSION: &str = "2.1.90";

// ── Beta exclusion tracking (for long-context retry logic) ──

use std::collections::{BTreeSet, HashMap};
use std::sync::{LazyLock, Mutex};

static EXCLUDED_BETAS: LazyLock<Mutex<HashMap<String, BTreeSet<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Process-stable session ID — sent as `x-claude-code-session-id` header.
static SESSION_ID: LazyLock<String> = LazyLock::new(generate_uuid_v4_hex);

pub fn is_beta_excluded(model: &str, beta: &str) -> bool {
    match EXCLUDED_BETAS.lock() {
        Ok(store) => store
            .get(model)
            .is_some_and(|set| set.contains(beta)),
        Err(_) => false,
    }
}

pub fn record_excluded_beta(model: &str, beta: &str) {
    if let Ok(mut store) = EXCLUDED_BETAS.lock() {
        let entry = store.entry(model.to_string()).or_default();
        entry.insert(beta.to_string());
    }
}

/// Return `REQUIRED_BETAS` filtered by model-specific and explicit exclusions.
pub fn get_model_betas<'a>(model: &str, excluded: Option<&BTreeSet<String>>) -> Vec<&'a str> {
    let mut betas: Vec<&str> = REQUIRED_BETAS
        .iter()
        .filter(|beta| {
            excluded.is_none_or(|ex| !ex.contains(**beta))
                && !is_beta_excluded(model, beta)
        })
        .copied()
        .collect();

    // 4.6 모델에 effort beta 추가 (opencode-claude-auth modelOverrides 패턴)
    let lower = model.to_lowercase();
    if lower.contains("4-6") {
        let effort_beta = "effort-2025-11-24";
        if !betas.contains(&effort_beta)
            && excluded.is_none_or(|ex| !ex.contains(effort_beta))
            && !is_beta_excluded(model, effort_beta)
        {
            betas.push(effort_beta);
        }
    }

    betas
}

// ── Header transforms (transform.ts: setOAuthHeaders, mergeBetaHeaders) ──

fn header_value(value: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(value).map_err(|error| ApiError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: Some(format!("invalid header value: {error}")),
        body: String::new(),
        retryable: false,
        retry_after_secs: None,
    })
}

fn cli_version() -> String {
    match std::env::var("ANTHROPIC_CLI_VERSION") {
        Ok(version) if !version.is_empty() => version,
        _ => DEFAULT_CLAUDE_CLI_VERSION.to_string(),
    }
}

fn user_agent() -> String {
    match std::env::var("ANTHROPIC_USER_AGENT") {
        Ok(user_agent) if !user_agent.is_empty() => user_agent,
        _ => format!("claude-cli/{} (external, cli)", cli_version()),
    }
}

fn generate_uuid_v4_hex() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read as _;
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0_u64, |d| d.as_secs())
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(u64::from(std::process::id()));
        let mut state = seed;
        for byte in &mut bytes {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *byte = state.to_le_bytes()[4];
        }
    }
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Merge model-aware betas with any existing `anthropic-beta` header value.
pub fn merge_beta_headers(
    headers: &HeaderMap,
    model: &str,
    excluded_betas: Option<&BTreeSet<String>>,
) -> String {
    let existing = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let mut betas: Vec<&str> = get_model_betas(model, excluded_betas);
    for beta in existing.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !betas.contains(&beta) {
            betas.push(beta);
        }
    }
    betas.join(",")
}

pub fn set_oauth_headers(
    headers: &mut HeaderMap,
    access_token: &str,
    model: &str,
    excluded_betas: Option<&BTreeSet<String>>,
) -> Result<(), ApiError> {
    headers.insert(
        "authorization",
        header_value(&format!("Bearer {access_token}"))?,
    );
    let merged = merge_beta_headers(headers, model, excluded_betas);
    headers.insert("anthropic-beta", header_value(&merged)?);
    headers.insert("user-agent", header_value(&user_agent())?);
    headers.insert("x-app", header_value("cli")?);
    headers.insert(
        "x-client-request-id",
        header_value(&generate_uuid_v4_hex())?,
    );
    headers.insert(
        "x-claude-code-session-id",
        header_value(&SESSION_ID)?,
    );
    headers.remove("x-api-key");
    Ok(())
}

// ── URL rewrite (transform.ts: rewriteUrl) ──

/// Append `?beta=true` to a `/v1/messages` URL.
/// Matches `transform.ts: rewriteUrl()`.
pub fn rewrite_url(url: &str) -> String {
    if url.contains("/v1/messages") && !url.contains("beta=") {
        if url.contains('?') {
            format!("{url}&beta=true")
        } else {
            format!("{url}?beta=true")
        }
    } else {
        url.to_string()
    }
}

// ── Tool name transforms (transform.ts: prefixToolNames, stripToolPrefix) ──

/// Add `TOOL_PREFIX` to tool names in the request body.
/// Matches `transform.ts: prefixToolNames()`.
pub fn prefix_tool_names(request: &mut MessageRequest) {
    if let Some(tools) = &mut request.tools {
        for tool in tools.iter_mut() {
            if !tool.name.starts_with(TOOL_PREFIX) {
                tool.name = format!("{TOOL_PREFIX}{}", tool.name);
            }
        }
    }

    for message in &mut request.messages {
        for block in &mut message.content {
            if let InputContentBlock::ToolUse { name, .. } = block
                && !name.starts_with(TOOL_PREFIX)
            {
                *name = format!("{TOOL_PREFIX}{name}");
            }
        }
    }
}

/// Strip `TOOL_PREFIX` from tool names in streaming response text.
/// Matches `transform.ts: stripToolPrefix()`.
pub fn strip_tool_prefix(text: &str) -> String {
    text.replace("\"name\": \"mcp_", "\"name\": \"")
        .replace("\"name\":\"mcp_", "\"name\":\"")
}

// ── Token refresh request body (index.ts: token refresh) ──

/// Build JSON body for token refresh request.
/// Matches `index.ts` token refresh logic.
pub fn token_refresh_body(refresh_token: &str) -> serde_json::Value {
    serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
    })
}

/// Headers for token refresh request. Matches `index.ts`.
pub fn token_refresh_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("content-type", "application/json"),
        ("accept", "application/json, text/plain, */*"),
        ("user-agent", "axios/1.13.6"),
    ]
}

// ── Billing header (opencode-claude-auth signing.ts) ──

const BILLING_SALT: &str = "59cf53e54c78";

pub fn extract_first_user_text(request: &MessageRequest) -> String {
    for msg in &request.messages {
        if msg.role == "user" {
            for block in &msg.content {
                if let InputContentBlock::Text { text } = block {
                    return text.clone();
                }
            }
        }
    }
    String::new()
}

fn compute_cch(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = hasher.finalize();
    format!("{hash:x}").chars().take(5).collect()
}

fn compute_version_suffix(text: &str, version: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let sampled: String = [4, 7, 20]
        .iter()
        .map(|&i| chars.get(i).copied().unwrap_or('0'))
        .collect();
    let input = format!("{BILLING_SALT}{sampled}{version}");
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    format!("{hash:x}").chars().take(3).collect()
}

pub fn build_billing_header(request: &MessageRequest) -> String {
    let version = cli_version();
    let text = extract_first_user_text(request);
    let suffix = compute_version_suffix(&text, &version);
    let cch = compute_cch(&text);
    format!(
        "x-anthropic-billing-header: cc_version={version}.{suffix}; cc_entrypoint=cli; cch={cch};"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_betas_deduplicates() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("oauth-2025-04-20,custom-beta"),
        );
        let result = merge_beta_headers(&headers, "test-model", None);
        assert!(result.contains("claude-code-20250219"));
        assert!(result.contains("oauth-2025-04-20"));
        assert!(result.contains("interleaved-thinking-2025-05-14"));
        assert!(result.contains("prompt-caching-scope-2026-01-05"));
        assert!(result.contains("context-management-2025-06-27"));
        assert!(result.contains("custom-beta"));
        assert_eq!(
            result.matches("oauth-2025-04-20").count(),
            1
        );
    }

    #[test]
    fn rewrite_url_adds_beta() {
        assert_eq!(
            rewrite_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com/v1/messages?beta=true"
        );
        // Already has beta
        assert_eq!(
            rewrite_url("https://api.anthropic.com/v1/messages?beta=true"),
            "https://api.anthropic.com/v1/messages?beta=true"
        );
        // Non-messages URL
        assert_eq!(
            rewrite_url("https://api.anthropic.com/v1/models"),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[test]
    fn prefix_and_strip_tool_names() {
        let mut request = MessageRequest {
            model: "test".to_string(),
            max_tokens: 64,
            messages: vec![],
            system: None,
            tools: Some(vec![crate::types::ToolDefinition {
                name: "bash".to_string(),
                description: None,
                input_schema: serde_json::json!({}),
            }]),
            tool_choice: None,
            stream: false,
        };
        prefix_tool_names(&mut request);
        assert_eq!(
            request.tools.as_ref().and_then(|t| t.first()).map(|t| t.name.as_str()),
            Some("mcp_bash")
        );

        // Strip from response text
        let text = r#"{"name": "mcp_bash", "id": "123"}"#;
        assert_eq!(strip_tool_prefix(text), r#"{"name": "bash", "id": "123"}"#);
    }
}
