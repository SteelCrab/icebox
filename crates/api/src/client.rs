use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};

use crate::error::ApiError;
use crate::oauth_transform;
use crate::sse::SseParser;
use crate::types::{MessageRequest, MessageResponse, StreamEvent};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_RETRIES: u32 = 5;
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(60);

const BETA_FEATURES: &str = "interleaved-thinking-2025-05-14";
const LONG_CONTEXT_BETAS: &[&str] = &["context-1m-2025-08-07", "interleaved-thinking-2025-05-14"];

fn is_long_context_error(body: &str) -> bool {
    body.contains("Extra usage is required for long context requests")
        || body.contains("long context beta is not yet available")
}

fn next_long_context_beta_to_exclude(model: &str) -> Option<&'static str> {
    LONG_CONTEXT_BETAS
        .iter()
        .copied()
        .find(|beta| !oauth_transform::is_beta_excluded(model, beta))
}

fn record_excluded_beta(model: &str, beta: &str) {
    oauth_transform::record_excluded_beta(model, beta);
}

fn json_parse_error(context: &str, error: &serde_json::Error, body: &str) -> ApiError {
    ApiError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: Some(format!("{context}: {error}")),
        body: body.to_string(),
        retryable: false,
        retry_after_secs: None,
    }
}

fn api_error_from_response(status: u16, body: &str, retry_after_secs: Option<u64>) -> ApiError {
    let http_status =
        reqwest::StatusCode::from_u16(status).unwrap_or(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        });
    let retryable = matches!(status, 408 | 429 | 500 | 502 | 503 | 504 | 529);

    ApiError::Api {
        status: http_status,
        message,
        body: body.to_string(),
        retryable,
        retry_after_secs,
    }
}

fn header_value(value: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(value).map_err(|error| ApiError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: Some(format!("invalid header value: {error}")),
        body: String::new(),
        retryable: false,
        retry_after_secs: None,
    })
}

// ── Plugin Trait (opencode-claude-auth pattern) ──

/// Request transformation plugin for header/body injection.
///
/// Based on the `opencode-claude-auth` middleware pattern:
/// `buildRequestHeaders` + `transformBody` hooks.
pub trait RequestPlugin: Send + Sync {
    fn transform_headers(
        &self,
        headers: &mut HeaderMap,
        request: &MessageRequest,
    ) -> Result<(), ApiError>;

    fn transform_body(&self, body: &mut MessageRequest) -> Result<(), ApiError> {
        let _ = body;
        Ok(())
    }
}

// ── Built-in Plugins ──

struct ApiKeyPlugin {
    api_key: String,
}

impl RequestPlugin for ApiKeyPlugin {
    fn transform_headers(
        &self,
        headers: &mut HeaderMap,
        _request: &MessageRequest,
    ) -> Result<(), ApiError> {
        headers.insert("x-api-key", header_value(&self.api_key)?);
        headers.insert("anthropic-beta", HeaderValue::from_static(BETA_FEATURES));
        Ok(())
    }
}

/// OAuth plugin — delegates to `oauth_transform` module (ported from opencode-anthropic-auth).
struct OAuthPlugin {
    bearer_token: String,
}

impl RequestPlugin for OAuthPlugin {
    fn transform_headers(
        &self,
        headers: &mut HeaderMap,
        request: &MessageRequest,
    ) -> Result<(), ApiError> {
        oauth_transform::set_oauth_headers(headers, &self.bearer_token, &request.model, None)
    }

    fn transform_body(&self, body: &mut MessageRequest) -> Result<(), ApiError> {
        oauth_transform::prefix_tool_names(body);

        let billing = oauth_transform::build_billing_header(body);
        let identity = "You are Claude Code, Anthropic's official CLI for Claude.";
        let prefix = format!("{billing}\n\n{identity}");

        match &mut body.system {
            Some(existing) => {
                *existing = format!("{prefix}\n\n{existing}");
            }
            None => {
                body.system = Some(prefix);
            }
        }

        Ok(())
    }
}

// ── Auth & Client ──

#[derive(Debug, Clone)]
pub enum AuthMethod {
    ApiKey(String),
    Bearer(String),
    Combined { api_key: String, bearer: String },
}

#[derive(Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    auth: AuthMethod,
    base_url: String,
    max_retries: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    plugins: Arc<[Box<dyn RequestPlugin>]>,
}

impl fmt::Debug for AnthropicClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicClient")
            .field("auth", &self.auth)
            .field("base_url", &self.base_url)
            .field("max_retries", &self.max_retries)
            .field("plugins", &format!("[{} plugin(s)]", self.plugins.len()))
            .finish()
    }
}

fn build_plugins(auth: &AuthMethod) -> Arc<[Box<dyn RequestPlugin>]> {
    let plugins: Vec<Box<dyn RequestPlugin>> = match auth {
        AuthMethod::ApiKey(api_key) => {
            vec![Box::new(ApiKeyPlugin {
                api_key: api_key.clone(),
            })]
        }
        AuthMethod::Bearer(token) | AuthMethod::Combined { bearer: token, .. } => {
            vec![Box::new(OAuthPlugin {
                bearer_token: token.clone(),
            })]
        }
    };
    Arc::from(plugins)
}

impl AnthropicClient {
    /// Create from ANTHROPIC_API_KEY env var. Returns error if not set.
    pub fn from_env() -> Result<Self, ApiError> {
        let api_key = match std::env::var("ANTHROPIC_API_KEY") {
            Ok(key) if !key.is_empty() => key,
            _ => return Err(ApiError::MissingApiKey),
        };
        let base_url = match std::env::var("ANTHROPIC_BASE_URL") {
            Ok(url) if !url.is_empty() => url,
            _ => DEFAULT_BASE_URL.to_string(),
        };
        let auth = AuthMethod::ApiKey(api_key);
        let plugins = build_plugins(&auth);
        Ok(Self {
            http: reqwest::Client::new(),
            auth,
            base_url,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            plugins,
        })
    }

    /// Create with a plain API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        let auth = AuthMethod::ApiKey(api_key.into());
        let plugins = build_plugins(&auth);
        Self {
            http: reqwest::Client::new(),
            auth,
            base_url: DEFAULT_BASE_URL.to_string(),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            plugins,
        }
    }

    /// Create with a bearer token (OAuth).
    #[must_use]
    pub fn from_bearer(token: impl Into<String>) -> Self {
        let auth = AuthMethod::Bearer(token.into());
        let plugins = build_plugins(&auth);
        Self {
            http: reqwest::Client::new(),
            auth,
            base_url: DEFAULT_BASE_URL.to_string(),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            plugins,
        }
    }

    /// Create with combined API key + bearer token.
    #[must_use]
    pub fn from_combined(api_key: impl Into<String>, bearer: impl Into<String>) -> Self {
        let auth = AuthMethod::Combined {
            api_key: api_key.into(),
            bearer: bearer.into(),
        };
        let plugins = build_plugins(&auth);
        Self {
            http: reqwest::Client::new(),
            auth,
            base_url: DEFAULT_BASE_URL.to_string(),
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            plugins,
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Exchange an OAuth authorization code for tokens.
    pub async fn exchange_oauth_code(
        &self,
        token_url: &str,
        params: &[(String, String)],
    ) -> Result<serde_json::Value, ApiError> {
        let response = self
            .http
            .post(token_url)
            .header("content-type", "application/x-www-form-urlencoded")
            .form(params)
            .send()
            .await?;
        let body = response.json::<serde_json::Value>().await?;
        Ok(body)
    }

    /// Refresh an OAuth token.
    pub async fn refresh_oauth_token(
        &self,
        token_url: &str,
        params: &[(String, String)],
    ) -> Result<serde_json::Value, ApiError> {
        self.exchange_oauth_code(token_url, params).await
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        let non_streaming = MessageRequest {
            stream: false,
            ..request.clone()
        };

        let response = self.send_with_retry(&non_streaming).await?;
        let body = response.text().await.unwrap_or_default();
        let parsed = serde_json::from_str::<MessageResponse>(&body).map_err(|error| {
            json_parse_error("failed to parse API response JSON", &error, &body)
        })?;
        Ok(parsed)
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageStream, ApiError> {
        let response = self
            .send_with_retry(&request.clone().with_streaming())
            .await?;
        Ok(MessageStream {
            response,
            parser: SseParser::new(),
            pending: VecDeque::new(),
            done: false,
        })
    }

    #[must_use]
    pub fn bearer_token(&self) -> Option<&str> {
        match &self.auth {
            AuthMethod::Bearer(token) => Some(token.as_str()),
            AuthMethod::Combined { bearer, .. } => Some(bearer.as_str()),
            AuthMethod::ApiKey(_) => None,
        }
    }

    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        match &self.auth {
            AuthMethod::ApiKey(api_key) | AuthMethod::Combined { api_key, .. } => {
                Some(api_key.as_str())
            }
            AuthMethod::Bearer(_) => None,
        }
    }

    fn messages_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{base}/v1/messages");
        match &self.auth {
            AuthMethod::Bearer(_) | AuthMethod::Combined { .. } => {
                oauth_transform::rewrite_url(&url)
            }
            AuthMethod::ApiKey(_) => url,
        }
    }

    async fn send_raw(&self, request: &MessageRequest) -> Result<reqwest::Response, ApiError> {
        let url = self.messages_url();
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );

        let mut body = request.clone();
        for plugin in self.plugins.iter() {
            plugin.transform_headers(&mut headers, request)?;
            plugin.transform_body(&mut body)?;
        }

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;
        Ok(response)
    }

    async fn send_with_retry(
        &self,
        request: &MessageRequest,
    ) -> Result<reqwest::Response, ApiError> {
        let mut last_error: Option<ApiError> = None;
        let mut next_wait: Option<Duration> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let wait = match next_wait.take() {
                    Some(wait) => wait,
                    None => self.backoff_for_attempt(attempt),
                };
                tokio::time::sleep(wait).await;
            }

            match self.send_raw(request).await {
                Ok(response) => match expect_success(response).await {
                    Ok(response) => return Ok(response),
                    Err(error) => {
                        if let ApiError::Api {
                            status, ref body, ..
                        } = error
                            && self.retry_without_long_context_beta(
                                request,
                                status.as_u16(),
                                body,
                            )
                        {
                            continue;
                        }

                        // 401 single retry — credentials may have expired mid-session
                        if let ApiError::Api { status, .. } = &error
                            && status.as_u16() == 401
                            && attempt == 0
                        {
                            last_error = Some(error);
                            continue;
                        }

                        if error.is_retryable() && attempt < self.max_retries {
                            if let ApiError::Api {
                                retry_after_secs: Some(retry_after_secs),
                                ..
                            } = &error
                            {
                                next_wait = Some(Duration::from_secs(*retry_after_secs));
                            }
                            last_error = Some(error);
                            continue;
                        }
                        return Err(error);
                    }
                },
                Err(error) if error.is_retryable() && attempt < self.max_retries => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        match last_error {
            Some(error) => Err(ApiError::RetriesExhausted {
                attempts: self.max_retries + 1,
                last_error: Box::new(error),
            }),
            None => Err(ApiError::MissingApiKey),
        }
    }

    fn retry_without_long_context_beta(
        &self,
        request: &MessageRequest,
        status: u16,
        body: &str,
    ) -> bool {
        if !matches!(status, 400 | 429) || !is_long_context_error(body) {
            return false;
        }

        match next_long_context_beta_to_exclude(&request.model) {
            Some(beta) => {
                record_excluded_beta(&request.model, beta);
                true
            }
            None => false,
        }
    }

    #[allow(clippy::manual_unwrap_or)]
    fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = match 1_u64.checked_shl(attempt.saturating_sub(1)) {
            Some(multiplier) => multiplier,
            None => u64::MAX,
        };
        let multiplier_u32 = if multiplier > u64::from(u32::MAX) {
            u32::MAX
        } else {
            multiplier as u32
        };

        self.initial_backoff
            .checked_mul(multiplier_u32)
            .map_or(self.max_backoff, |duration| duration.min(self.max_backoff))
    }
}

// ── Streaming ──

pub struct MessageStream {
    response: reqwest::Response,
    parser: SseParser,
    pending: VecDeque<StreamEvent>,
    done: bool,
}

impl MessageStream {
    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }

            if self.done {
                let remaining = self.parser.finish()?;
                self.pending.extend(remaining);
                return Ok(self.pending.pop_front());
            }

            match self.response.chunk().await? {
                Some(chunk) => {
                    self.pending.extend(self.parser.push(&chunk)?);
                }
                None => {
                    self.done = true;
                }
            }
        }
    }
}

// ── Response Helpers ──

async fn expect_success(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let retry_after_secs = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    let body: String = response.text().await.unwrap_or_default();

    eprintln!("[icebox-debug] HTTP {status}");
    eprintln!(
        "[icebox-debug] Body: {}",
        body.chars().take(500).collect::<String>()
    );

    Err(api_error_from_response(
        status.as_u16(),
        &body,
        retry_after_secs,
    ))
}
