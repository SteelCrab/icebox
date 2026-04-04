use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CLAUDE_CODE_PRIMARY_SERVICE: &str = "Claude Code-credentials";
const CLAUDE_CODE_REFRESH_THRESHOLD_SECS: u64 = 60;
const CLAUDE_CODE_TOKEN_URL: &str = "https://claude.ai/v1/oauth/token";
const CREDENTIAL_CACHE_TTL: Duration = Duration::from_secs(30);

// --- Credential TTL cache (30s, opencode-claude-auth pattern) ---

struct CachedCredentials {
    access_token: String,
    expires_at: u64,
    cached_at: Instant,
}

static CREDENTIAL_CACHE: LazyLock<Mutex<Option<CachedCredentials>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn get_cached_access_token() -> Result<Option<String>> {
    let now = Instant::now();

    if let Ok(cache) = CREDENTIAL_CACHE.lock()
        && let Some(cached) = cache.as_ref()
        && now.saturating_duration_since(cached.cached_at) < CREDENTIAL_CACHE_TTL
        && cached.expires_at > now_unix() + 60
    {
        return Ok(Some(cached.access_token.clone()));
    }

    let auth = AuthSource::resolve()?;
    let token = match &auth {
        AuthSource::BearerToken(t) => Some(t.clone()),
        AuthSource::ApiKeyAndBearer {
            bearer_token: t, ..
        } => Some(t.clone()),
        AuthSource::ApiKey(_) | AuthSource::None => None,
    };

    if let Some(ref t) = token
        && let Ok(mut cache) = CREDENTIAL_CACHE.lock()
    {
        *cache = Some(CachedCredentials {
            access_token: t.clone(),
            expires_at: now_unix() + 3600,
            cached_at: now,
        });
    }

    Ok(token)
}

// --- Data structures ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub callback_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    pub scopes: Vec<String>,
}

/// Default OAuth provider — Claude.ai (Max plan).
///
/// Endpoints and scopes sourced from opencode-anthropic-auth.
/// The client_id is shared across Claude Code, claurst, opencode, and icebox.
impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
            authorize_url: "https://claude.ai/oauth/authorize".to_string(),
            token_url: "https://platform.claude.com/v1/oauth/token".to_string(),
            callback_port: Some(4545),
            redirect_uri: None,
            scopes: vec![
                "org:create_api_key".to_string(),
                "user:profile".to_string(),
                "user:inference".to_string(),
                "user:sessions:claude_code".to_string(),
                "user:mcp_servers".to_string(),
                "user:file_upload".to_string(),
            ],
        }
    }
}

impl OAuthConfig {
    /// Console OAuth variant (API key creation via console.anthropic.com).
    #[must_use]
    pub fn console() -> Self {
        Self {
            authorize_url: "https://platform.claude.com/oauth/authorize".to_string(),
            ..Self::default()
        }
    }

    /// Code-display OAuth: browser shows code, user pastes it (no localhost server).
    #[must_use]
    pub fn code_display() -> Self {
        Self {
            client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
            authorize_url: "https://claude.ai/oauth/authorize".to_string(),
            token_url: "https://platform.claude.com/v1/oauth/token".to_string(),
            callback_port: None,
            redirect_uri: Some("https://platform.claude.com/oauth/code/callback".to_string()),
            scopes: vec![
                "user:profile".to_string(),
                "user:inference".to_string(),
                "user:sessions:claude_code".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken", skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeCredentials {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: u64,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(
        rename = "subscriptionType",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub subscription_type: Option<String>,
    #[serde(
        rename = "rateLimitTier",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub rate_limit_tier: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeAccount {
    pub label: String,
    pub source: String,
    pub credentials: ClaudeCodeCredentials,
}

#[derive(Debug, Clone)]
pub struct PkceCodePair {
    pub verifier: String,
    pub challenge: String,
    pub challenge_method: String,
}

#[derive(Debug, Clone)]
pub struct OAuthCallbackParams {
    pub code: String,
    pub state: String,
}

/// Authentication source: API key, bearer token, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSource {
    None,
    ApiKey(String),
    BearerToken(String),
    ApiKeyAndBearer {
        api_key: String,
        bearer_token: String,
    },
}

impl AuthSource {
    /// Resolve authentication from environment variables, saved credentials, and keychain.
    /// Priority: ANTHROPIC_API_KEY > ANTHROPIC_AUTH_TOKEN > Claude Code keychain > saved icebox OAuth.
    pub fn resolve() -> Result<Self> {
        // 1. API key env
        if let Some(api_key) = read_env_non_empty("ANTHROPIC_API_KEY") {
            return match read_env_non_empty("ANTHROPIC_AUTH_TOKEN") {
                Some(bearer) => Ok(Self::ApiKeyAndBearer {
                    api_key,
                    bearer_token: bearer,
                }),
                None => Ok(Self::ApiKey(api_key)),
            };
        }

        // 2. Auth token env
        if let Some(bearer) = read_env_non_empty("ANTHROPIC_AUTH_TOKEN") {
            return Ok(Self::BearerToken(bearer));
        }

        // 3. Claude Code credentials — PRIMARY
        if let Some(mut account) = load_active_claude_code_account()? {
            refresh_claude_code_account_if_needed(&mut account)?;
            sync_icebox_oauth_from_claude_code(&account)?;
            return Ok(Self::BearerToken(account.credentials.access_token));
        }

        // 4. icebox saved OAuth — FALLBACK
        match load_oauth_credentials()? {
            Some(token_set) => {
                if is_token_expired(&token_set) {
                    match refresh_saved_token(&OAuthConfig::default()) {
                        Ok(refreshed) => {
                            eprintln!("OAuth token refreshed successfully.");
                            Ok(Self::BearerToken(refreshed.access_token))
                        }
                        Err(_) => match refresh_saved_token(&OAuthConfig::console()) {
                            Ok(refreshed) => {
                                eprintln!("OAuth token refreshed (console).");
                                Ok(Self::BearerToken(refreshed.access_token))
                            }
                            Err(e) => anyhow::bail!(
                                "OAuth token expired and refresh failed: {e}\nRun `icebox login` to re-authenticate."
                            ),
                        },
                    }
                } else {
                    Ok(Self::BearerToken(token_set.access_token))
                }
            }
            None => anyhow::bail!(
                "No API key found.\n\
                 Option 1: Install Claude Code (`claude`) and log in\n\
                 Option 2: Set ANTHROPIC_API_KEY env var\n\
                 Option 3: Run `icebox login`"
            ),
        }
    }

    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::ApiKey(k) | Self::ApiKeyAndBearer { api_key: k, .. } => Some(k),
            Self::None | Self::BearerToken(_) => None,
        }
    }

    #[must_use]
    pub fn bearer_token(&self) -> Option<&str> {
        match self {
            Self::BearerToken(t)
            | Self::ApiKeyAndBearer {
                bearer_token: t, ..
            } => Some(t),
            Self::None | Self::ApiKey(_) => None,
        }
    }

    /// Apply auth headers to a reqwest request builder.
    pub fn apply(&self, mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = self.api_key() {
            builder = builder.header("x-api-key", key);
        }
        if let Some(token) = self.bearer_token() {
            builder = builder.bearer_auth(token);
        }
        builder
    }
}

impl From<&ClaudeCodeCredentials> for OAuthTokenSet {
    fn from(value: &ClaudeCodeCredentials) -> Self {
        Self {
            access_token: value.access_token.clone(),
            refresh_token: Some(value.refresh_token.clone()),
            expires_at: Some(normalize_expiry_secs(value.expires_at)),
            scopes: value.scopes.clone(),
        }
    }
}

#[must_use]
pub fn claude_code_selection_path() -> PathBuf {
    config_home().join("claude-account-source.txt")
}

pub fn load_claude_code_accounts() -> Result<Vec<ClaudeCodeAccount>> {
    #[cfg(target_os = "macos")]
    {
        let services = list_claude_code_keychain_services()?;
        let mut accounts = Vec::new();
        for service in services {
            if let Some(credentials) = read_claude_code_keychain_credentials(&service)? {
                accounts.push(ClaudeCodeAccount {
                    label: build_claude_account_label(&credentials),
                    source: service,
                    credentials,
                });
            }
        }

        if accounts.is_empty()
            && let Some(file_account) = load_claude_code_file_account()?
        {
            accounts.push(file_account);
        }

        dedupe_claude_account_labels(&mut accounts);
        Ok(accounts)
    }

    #[cfg(not(target_os = "macos"))]
    {
        match load_claude_code_file_account()? {
            Some(account) => Ok(vec![account]),
            None => Ok(Vec::new()),
        }
    }
}

pub fn load_active_claude_code_account() -> Result<Option<ClaudeCodeAccount>> {
    let accounts = load_claude_code_accounts()?;
    if accounts.is_empty() {
        return Ok(None);
    }

    if let Some(source) = read_env_non_empty("ICEBOX_CLAUDE_ACCOUNT_SOURCE")
        && let Some(account) = accounts.iter().find(|account| account.source == source)
    {
        return Ok(Some(account.clone()));
    }

    if let Some(source) = load_persisted_claude_code_source()?
        && let Some(account) = accounts.iter().find(|account| account.source == source)
    {
        return Ok(Some(account.clone()));
    }

    Ok(accounts.into_iter().next())
}

pub fn sync_icebox_oauth_from_claude_code(account: &ClaudeCodeAccount) -> Result<()> {
    let token_set = OAuthTokenSet::from(&account.credentials);
    save_oauth_credentials(&token_set)
}

pub fn refresh_claude_code_account_if_needed(account: &mut ClaudeCodeAccount) -> Result<()> {
    let expires_at = normalize_expiry_secs(account.credentials.expires_at);
    if expires_at > now_unix() + CLAUDE_CODE_REFRESH_THRESHOLD_SECS {
        return Ok(());
    }

    let refreshed = match refresh_claude_code_credentials(&account.credentials.refresh_token) {
        Ok(credentials) => credentials,
        Err(oauth_error) => {
            refresh_claude_code_via_cli()?;
            read_claude_code_account_by_source(&account.source)?
                .with_context(|| {
                    format!(
                        "Claude Code credentials refreshed via CLI but source '{}' was not found",
                        account.source
                    )
                })
                .map_err(|_| anyhow::anyhow!("direct Claude OAuth refresh failed: {oauth_error}"))?
        }
    };

    write_back_claude_code_credentials(&account.source, &refreshed)?;
    account.credentials = refreshed;
    save_claude_code_source(&account.source)?;
    Ok(())
}

fn normalize_expiry_secs(expires_at: u64) -> u64 {
    if expires_at > 1_000_000_000_000 {
        expires_at / 1000
    } else {
        expires_at
    }
}

fn build_claude_account_label(credentials: &ClaudeCodeCredentials) -> String {
    match credentials.subscription_type.as_deref() {
        Some(subscription) if !subscription.is_empty() => {
            let mut chars = subscription.chars();
            match chars.next() {
                Some(first) => {
                    let rest: String = chars.collect();
                    format!("Claude {}{}", first.to_uppercase(), rest)
                }
                None => "Claude".to_string(),
            }
        }
        _ => "Claude".to_string(),
    }
}

fn dedupe_claude_account_labels(accounts: &mut [ClaudeCodeAccount]) {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for account in accounts.iter() {
        let entry = counts.entry(account.label.clone()).or_default();
        *entry += 1;
    }

    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for account in accounts.iter_mut() {
        let total = counts.get(&account.label).copied().unwrap_or(1);
        if total <= 1 {
            continue;
        }
        let entry = seen.entry(account.label.clone()).or_default();
        *entry += 1;
        account.label = format!("{} {}", account.label, entry);
    }
}

fn load_persisted_claude_code_source() -> Result<Option<String>> {
    let path = claude_code_selection_path();
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn save_claude_code_source(source: &str) -> Result<()> {
    let path = claude_code_selection_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
    }
    fs::write(&path, source).with_context(|| format!("failed to write {}", path.display()))
}

fn read_claude_code_account_by_source(source: &str) -> Result<Option<ClaudeCodeCredentials>> {
    if source == "file" {
        return load_claude_code_file_credentials();
    }
    read_claude_code_keychain_credentials(source)
}

#[cfg(target_os = "macos")]
fn list_claude_code_keychain_services() -> Result<Vec<String>> {
    let output = Command::new("security")
        .arg("dump-keychain")
        .output()
        .context("failed to inspect macOS Keychain")?;

    if !output.status.success() {
        return Ok(vec![CLAUDE_CODE_PRIMARY_SERVICE.to_string()]);
    }

    let dump = String::from_utf8(output.stdout).context("Keychain dump was not valid UTF-8")?;
    let pattern = Regex::new(r#""Claude Code-credentials(?:-[0-9a-f]+)?""#)
        .context("failed to build Keychain service matcher")?;

    let mut services = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if dump.contains(CLAUDE_CODE_PRIMARY_SERVICE) {
        services.push(CLAUDE_CODE_PRIMARY_SERVICE.to_string());
        seen.insert(CLAUDE_CODE_PRIMARY_SERVICE.to_string());
    }

    for matched in pattern.find_iter(&dump) {
        let service = matched.as_str().trim_matches('"').to_string();
        if seen.insert(service.clone()) {
            services.push(service);
        }
    }

    if services.is_empty() {
        services.push(CLAUDE_CODE_PRIMARY_SERVICE.to_string());
    }

    Ok(services)
}

#[cfg(not(target_os = "macos"))]
fn list_claude_code_keychain_services() -> Result<Vec<String>> {
    Ok(vec![])
}

#[cfg(target_os = "macos")]
fn read_claude_code_keychain_credentials(service: &str) -> Result<Option<ClaudeCodeCredentials>> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()
        .with_context(|| format!("failed to read Keychain service '{service}'"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let raw =
        String::from_utf8(output.stdout).context("Keychain credential was not valid UTF-8")?;
    parse_claude_code_credentials(raw.trim())
}

#[cfg(not(target_os = "macos"))]
fn read_claude_code_keychain_credentials(_service: &str) -> Result<Option<ClaudeCodeCredentials>> {
    Ok(None)
}

fn load_claude_code_file_account() -> Result<Option<ClaudeCodeAccount>> {
    match load_claude_code_file_credentials()? {
        Some(credentials) => Ok(Some(ClaudeCodeAccount {
            label: build_claude_account_label(&credentials),
            source: "file".to_string(),
            credentials,
        })),
        None => Ok(None),
    }
}

fn load_claude_code_file_credentials() -> Result<Option<ClaudeCodeCredentials>> {
    let home = match std::env::var("HOME") {
        Ok(home) => home,
        Err(_) => return Ok(None),
    };
    let path = PathBuf::from(home)
        .join(".claude")
        .join(".credentials.json");
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_claude_code_credentials(&content)
}

fn parse_claude_code_credentials(raw: &str) -> Result<Option<ClaudeCodeCredentials>> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw).context("invalid Claude credentials JSON")?;
    let oauth = parsed.get("claudeAiOauth").unwrap_or(&parsed);

    if parsed.get("mcpOAuth").is_some() && oauth.get("accessToken").is_none() {
        return Ok(None);
    }

    let credentials: ClaudeCodeCredentials = match serde_json::from_value(oauth.clone()) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    Ok(Some(credentials))
}

/// Refresh token using JSON body — matches `opencode-anthropic-auth` pattern.
fn refresh_claude_code_credentials(refresh_token: &str) -> Result<ClaudeCodeCredentials> {
    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
    });

    let response = client
        .post(CLAUDE_CODE_TOKEN_URL)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/plain, */*")
        .header("user-agent", "axios/1.13.6")
        .json(&body)
        .send()
        .with_context(|| {
            format!("failed to refresh Claude Code token at {CLAUDE_CODE_TOKEN_URL}")
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        anyhow::bail!("Claude OAuth refresh failed ({status}): {body}");
    }

    let body: serde_json::Value = response
        .json()
        .context("invalid JSON in Claude OAuth refresh response")?;

    let access_token = body
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .context("missing access_token in Claude OAuth refresh response")?
        .to_string();

    let expires_in = body
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(36_000);

    let refreshed = ClaudeCodeCredentials {
        access_token,
        refresh_token: body
            .get("refresh_token")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| refresh_token.to_string(), ToString::to_string),
        expires_at: now_unix() + expires_in,
        scopes: body
            .get("scope")
            .and_then(serde_json::Value::as_str)
            .map(|scope| scope.split_whitespace().map(ToString::to_string).collect())
            .unwrap_or_default(),
        subscription_type: None,
        rate_limit_tier: None,
    };

    Ok(refreshed)
}

fn refresh_claude_code_via_cli() -> Result<()> {
    let mut attempt = 0_u8;
    while attempt < 2 {
        attempt += 1;
        let status = Command::new("claude")
            .args(["-p", ".", "--model", "haiku"])
            .current_dir(std::env::temp_dir())
            .env("TERM", "dumb")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("failed to execute `claude` for credential refresh")?;
        if status.success() {
            return Ok(());
        }
    }
    anyhow::bail!("`claude` refresh failed")
}

fn write_back_claude_code_credentials(
    source: &str,
    credentials: &ClaudeCodeCredentials,
) -> Result<()> {
    if source == "file" {
        return write_back_claude_code_file(credentials);
    }

    #[cfg(target_os = "macos")]
    {
        let account_name = keychain_account_name(source)?.unwrap_or_else(|| source.to_string());
        let raw = Command::new("security")
            .args(["find-generic-password", "-s", source, "-w"])
            .output()
            .with_context(|| format!("failed to read existing Keychain entry '{source}'"))?;
        if !raw.status.success() {
            anyhow::bail!("failed to read existing Keychain entry '{source}'");
        }
        let existing =
            String::from_utf8(raw.stdout).context("existing Keychain entry was not valid UTF-8")?;
        let updated = update_claude_credential_blob(existing.trim(), credentials)
            .context("failed to update stored Claude credentials")?;
        let status = Command::new("security")
            .args([
                "add-generic-password",
                "-s",
                source,
                "-a",
                &account_name,
                "-w",
                &updated,
                "-U",
            ])
            .status()
            .with_context(|| format!("failed to write updated Keychain entry '{source}'"))?;
        if !status.success() {
            anyhow::bail!("failed to update Keychain entry '{source}'");
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("Claude Code Keychain write-back is unsupported on this platform");
    }
}

fn write_back_claude_code_file(credentials: &ClaudeCodeCredentials) -> Result<()> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    let path = PathBuf::from(home)
        .join(".claude")
        .join(".credentials.json");
    let existing =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let updated = update_claude_credential_blob(&existing, credentials)
        .context("failed to update Claude credentials file payload")?;
    fs::write(&path, updated).with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&path, perms);
    }
    Ok(())
}

fn update_claude_credential_blob(raw: &str, credentials: &ClaudeCodeCredentials) -> Result<String> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(raw).context("invalid Claude credential blob")?;
    let target = match parsed.get_mut("claudeAiOauth") {
        Some(wrapper) => wrapper,
        None => &mut parsed,
    };
    let target_obj = target
        .as_object_mut()
        .context("Claude credential payload was not an object")?;

    target_obj.insert(
        "accessToken".to_string(),
        serde_json::Value::String(credentials.access_token.clone()),
    );
    target_obj.insert(
        "refreshToken".to_string(),
        serde_json::Value::String(credentials.refresh_token.clone()),
    );
    target_obj.insert(
        "expiresAt".to_string(),
        serde_json::Value::Number(serde_json::Number::from(credentials.expires_at)),
    );
    if !credentials.scopes.is_empty() {
        target_obj.insert(
            "scopes".to_string(),
            serde_json::Value::Array(
                credentials
                    .scopes
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    if let Some(subscription_type) = &credentials.subscription_type {
        target_obj.insert(
            "subscriptionType".to_string(),
            serde_json::Value::String(subscription_type.clone()),
        );
    }
    if let Some(rate_limit_tier) = &credentials.rate_limit_tier {
        target_obj.insert(
            "rateLimitTier".to_string(),
            serde_json::Value::String(rate_limit_tier.clone()),
        );
    }

    serde_json::to_string(&parsed).context("failed to serialize updated Claude credentials")
}

#[cfg(target_os = "macos")]
fn keychain_account_name(service: &str) -> Result<Option<String>> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", service])
        .output()
        .with_context(|| format!("failed to inspect Keychain entry '{service}'"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8(output.stdout).context("Keychain metadata was not valid UTF-8")?;
    let marker = "\"acct\"<blob>=\"";
    let start = match text.find(marker) {
        Some(pos) => pos + marker.len(),
        None => return Ok(None),
    };
    let rest = &text[start..];
    match rest.find('"') {
        Some(end) => Ok(Some(rest[..end].to_string())),
        None => Ok(None),
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_account_name(_service: &str) -> Result<Option<String>> {
    Ok(None)
}

// --- PKCE ---

pub fn generate_pkce_pair() -> PkceCodePair {
    let verifier = generate_random_string(43);
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    let challenge = base64url_encode(&hash);

    PkceCodePair {
        verifier,
        challenge,
        challenge_method: "S256".to_string(),
    }
}

pub fn generate_state() -> String {
    generate_random_string(32)
}

/// Build the authorization URL for the OAuth flow.
pub fn build_authorize_url(config: &OAuthConfig, pkce: &PkceCodePair, state: &str) -> String {
    let port = config.callback_port.unwrap_or(4545);
    let redirect_uri = format!("http://localhost:{port}/callback");
    let scopes = config.scopes.join(" ");

    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method={}",
        config.authorize_url,
        percent_encode(&config.client_id),
        percent_encode(&redirect_uri),
        percent_encode(&scopes),
        percent_encode(state),
        percent_encode(&pkce.challenge),
        percent_encode(&pkce.challenge_method),
    )
}

pub fn build_code_display_authorize_url(
    config: &OAuthConfig,
    code_challenge: &str,
    state: &str,
) -> String {
    let redirect_uri = config
        .redirect_uri
        .as_deref()
        .unwrap_or("https://platform.claude.com/oauth/code/callback");
    let scopes = config.scopes.join(" ");

    format!(
        "{}?code=true&response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        config.authorize_url,
        percent_encode(&config.client_id),
        percent_encode(redirect_uri),
        percent_encode(&scopes),
        percent_encode(code_challenge),
        percent_encode(state),
    )
}

pub fn exchange_code_json(
    config: &OAuthConfig,
    code: &str,
    code_verifier: &str,
    state: &str,
) -> Result<OAuthTokenSet> {
    let code = sanitize_auth_code(code);
    let redirect_uri = config
        .redirect_uri
        .as_deref()
        .unwrap_or("https://platform.claude.com/oauth/code/callback");

    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": code,
        "state": state,
        "client_id": config.client_id,
        "redirect_uri": redirect_uri,
        "code_verifier": code_verifier,
    });

    let resp = client
        .post(&config.token_url)
        .json(&body)
        .send()
        .with_context(|| format!("token exchange failed at {}", config.token_url))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().unwrap_or_default();
        anyhow::bail!("token exchange failed ({}): {}", status, body_text);
    }

    let token_resp: serde_json::Value = resp.json().context("invalid JSON in token response")?;

    let access_token = token_resp
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .context("no access_token in response")?
        .to_string();

    let refresh_token = token_resp
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .map(String::from);

    let expires_at = token_resp
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .map(|secs| now_unix() + secs);

    let scopes = token_resp
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_else(|| config.scopes.clone());

    let token_set = OAuthTokenSet {
        access_token,
        refresh_token,
        expires_at,
        scopes,
    };

    save_oauth_credentials(&token_set).context("failed to save credentials")?;

    Ok(token_set)
}

fn sanitize_auth_code(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.find('#') {
        Some(pos) => trimmed.chars().take(pos).collect(),
        None => trimmed.to_string(),
    }
}

/// Wait for the OAuth callback on a local TCP listener.
/// Returns the parsed callback parameters.
pub fn wait_for_oauth_callback(port: u16) -> Result<OAuthCallbackParams> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).with_context(|| format!("failed to bind to {addr}"))?;

    let (mut stream, _) = listener.accept().context("failed to accept connection")?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("failed to read request")?;

    // Parse query params from GET /callback?code=...&state=...
    let params = parse_callback_params(&request_line)?;

    // Send success response
    let html = "<html><body><h2>Authentication successful!</h2><p>You can close this tab.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    Ok(params)
}

fn parse_callback_params(request_line: &str) -> Result<OAuthCallbackParams> {
    // "GET /callback?code=xxx&state=yyy HTTP/1.1"
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("invalid HTTP request")?;

    let query = path
        .split_once('?')
        .map(|(_, q)| q)
        .context("no query parameters in callback")?;

    let mut code = None;
    let mut state = None;

    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            match key {
                "code" => code = Some(percent_decode(value)),
                "state" => state = Some(percent_decode(value)),
                _ => {}
            }
        }
    }

    Ok(OAuthCallbackParams {
        code: code.context("missing 'code' parameter in callback")?,
        state: state.context("missing 'state' parameter in callback")?,
    })
}

/// Build form parameters for exchanging an authorization code for tokens.
/// Matches claw-code's `OAuthTokenExchangeRequest::form_params()` exactly.
pub fn token_exchange_params(
    config: &OAuthConfig,
    code: &str,
    state: &str,
    pkce_verifier: &str,
) -> Vec<(String, String)> {
    let port = config.callback_port.unwrap_or(4545);
    vec![
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code.to_string()),
        (
            "redirect_uri".to_string(),
            format!("http://localhost:{port}/callback"),
        ),
        ("client_id".to_string(), config.client_id.clone()),
        ("code_verifier".to_string(), pkce_verifier.to_string()),
        ("state".to_string(), state.to_string()),
    ]
}

/// Build form parameters for refreshing a token.
pub fn token_refresh_params(config: &OAuthConfig, refresh_token: &str) -> Vec<(String, String)> {
    vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        ("client_id".to_string(), config.client_id.clone()),
        ("refresh_token".to_string(), refresh_token.to_string()),
    ]
}

/// Refresh an expired OAuth token using the refresh_token.
/// Uses a blocking reqwest call (safe to call from sync context before tokio runtime starts).
/// Follows claw-code's pattern: refresh → save → return new token set.
pub fn refresh_saved_token(config: &OAuthConfig) -> Result<OAuthTokenSet> {
    let token_set = load_oauth_credentials()?.context("no saved OAuth credentials to refresh")?;

    let refresh_token = token_set
        .refresh_token
        .as_deref()
        .context("token expired but no refresh_token available — run `icebox login`")?;

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": config.client_id,
        "refresh_token": refresh_token,
    });

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&config.token_url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/plain, */*")
        .header("user-agent", "axios/1.13.6")
        .json(&body)
        .send()
        .with_context(|| format!("failed to refresh token at {}", config.token_url))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        anyhow::bail!("token refresh failed ({}): {}", status, body);
    }

    let body: serde_json::Value = resp.json().context("invalid JSON in refresh response")?;

    let access_token = body
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .context("no access_token in refresh response")?
        .to_string();

    // Preserve old refresh_token if new one not provided (per OAuth spec)
    let new_refresh = body
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .or_else(|| token_set.refresh_token.clone());

    let new_expires_at = body
        .get("expires_at")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            body.get("expires_in")
                .and_then(serde_json::Value::as_u64)
                .map(|secs| now_unix() + secs)
        });

    let new_scopes = body
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_else(|| token_set.scopes.clone());

    let new_token_set = OAuthTokenSet {
        access_token,
        refresh_token: new_refresh,
        expires_at: new_expires_at,
        scopes: new_scopes,
    };

    save_oauth_credentials(&new_token_set).context("failed to save refreshed credentials")?;

    Ok(new_token_set)
}

// --- Credential storage ---

fn config_home() -> PathBuf {
    if let Some(path) = read_env_non_empty("ICEBOX_CONFIG_HOME") {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "linux")]
    if let Some(xdg) = read_env_non_empty("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("icebox");
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".icebox")
}

fn credentials_path() -> PathBuf {
    config_home().join("credentials.json")
}

pub fn load_oauth_credentials() -> Result<Option<OAuthTokenSet>> {
    let path = credentials_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read credentials: {}", path.display()))?;
    let root: serde_json::Value =
        serde_json::from_str(&content).context("invalid credentials JSON")?;

    match root.get("oauth") {
        Some(oauth_val) => {
            let token_set: OAuthTokenSet =
                serde_json::from_value(oauth_val.clone()).context("invalid oauth token data")?;
            Ok(Some(token_set))
        }
        None => Ok(None),
    }
}

pub fn save_oauth_credentials(token_set: &OAuthTokenSet) -> Result<()> {
    let path = credentials_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
    }

    // Preserve existing fields in the credentials file
    let mut root: serde_json::Value = if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let oauth_val = serde_json::to_value(token_set).context("failed to serialize token set")?;
    root["oauth"] = oauth_val;

    let json = serde_json::to_string_pretty(&root).context("failed to serialize credentials")?;

    // Atomic write
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("failed to rename to {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&path, perms);
    }

    Ok(())
}

pub fn clear_oauth_credentials() -> Result<()> {
    let path = credentials_path();
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
    let mut root: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(obj) = root.as_object_mut() {
        obj.remove("oauth");
    }

    let json = serde_json::to_string_pretty(&root).context("failed to serialize")?;
    fs::write(&path, &json).with_context(|| format!("failed to write {}", path.display()))?;

    Ok(())
}

pub fn is_token_expired(token_set: &OAuthTokenSet) -> bool {
    match token_set.expires_at {
        Some(expires_at) => expires_at <= now_unix(),
        None => false,
    }
}

// --- Encoding utilities ---

fn base64url_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity((data.len() * 4).div_ceil(3));

    for chunk in data.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0);
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);

        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);

        result.push(ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(n & 0x3F) as usize] as char);
        }
    }

    result
}

fn percent_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn percent_decode(s: &str) -> String {
    let mut result = Vec::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 2 < chars.len() {
            let hex: String = [chars[i + 1], chars[i + 2]].iter().collect();
            match u8::from_str_radix(&hex, 16) {
                Ok(byte) => {
                    result.push(byte);
                    i += 3;
                }
                Err(_) => {
                    result.extend_from_slice(chars[i].to_string().as_bytes());
                    i += 1;
                }
            }
        } else if chars[i] == '+' {
            result.push(b' ');
            i += 1;
        } else {
            result.extend_from_slice(chars[i].to_string().as_bytes());
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn generate_random_string(len: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

    // Read from /dev/urandom (available on macOS and Linux)
    let mut bytes = vec![0u8; len];
    if let Ok(mut file) = fs::File::open("/dev/urandom") {
        use std::io::Read as _;
        if file.read_exact(&mut bytes).is_ok() {
            return bytes
                .iter()
                .map(|b| CHARSET[(*b as usize) % CHARSET.len()] as char)
                .collect();
        }
    }

    // Fallback: time + pid (only if /dev/urandom unavailable)
    let seed = now_unix()
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(u64::from(std::process::id()));

    let mut state = seed;
    let mut result = String::with_capacity(len);

    for _ in 0..len {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let idx = ((state >> 33) as usize) % CHARSET.len();
        result.push(CHARSET[idx] as char);
    }

    result
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn read_env_non_empty(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(val) if !val.is_empty() => Some(val),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_pair_generates_valid_challenge() {
        let pair = generate_pkce_pair();
        assert!(!pair.verifier.is_empty());
        assert!(!pair.challenge.is_empty());
        assert_eq!(pair.challenge_method, "S256");
        // Verifier and challenge should differ
        assert_ne!(pair.verifier, pair.challenge);
    }

    #[test]
    fn state_is_nonempty() {
        let state = generate_state();
        assert!(!state.is_empty());
        assert!(state.len() >= 20);
    }

    #[test]
    fn percent_encode_roundtrip() {
        let original = "hello world/foo&bar=baz";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn base64url_encodes_correctly() {
        let data = b"hello";
        let encoded = base64url_encode(data);
        assert!(!encoded.is_empty());
        // base64url should not contain + or /
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }

    #[test]
    fn token_expiry_check() {
        let expired = OAuthTokenSet {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Some(1),
            scopes: vec![],
        };
        assert!(is_token_expired(&expired));

        let valid = OAuthTokenSet {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: Some(now_unix() + 3600),
            scopes: vec![],
        };
        assert!(!is_token_expired(&valid));

        let no_expiry = OAuthTokenSet {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };
        assert!(!is_token_expired(&no_expiry));
    }

    #[test]
    fn auth_source_api_key() {
        let auth = AuthSource::ApiKey("test-key".to_string());
        assert_eq!(auth.api_key(), Some("test-key"));
        assert_eq!(auth.bearer_token(), None);
    }

    #[test]
    fn auth_source_combined() {
        let auth = AuthSource::ApiKeyAndBearer {
            api_key: "key".to_string(),
            bearer_token: "token".to_string(),
        };
        assert_eq!(auth.api_key(), Some("key"));
        assert_eq!(auth.bearer_token(), Some("token"));
    }

    #[test]
    fn parse_callback_params_works() -> Result<()> {
        let request = "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\n";
        let params = parse_callback_params(request)?;
        assert_eq!(params.code, "abc123");
        assert_eq!(params.state, "xyz789");
        Ok(())
    }
}
