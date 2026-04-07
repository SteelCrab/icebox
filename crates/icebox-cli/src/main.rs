use anyhow::{Context, Result};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use icebox_task::store::TaskStore;
use icebox_tui::app::App;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::env;
use std::fs;
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // Subcommand dispatch
    match args.get(1).map(|s| s.as_str()) {
        Some("login") => return run_login(),
        Some("logout") => return run_logout(),
        Some("whoami") => return run_whoami(),
        Some("test-api") => return run_test_api(),
        Some("init") => return run_init(&args),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some(arg) if arg.starts_with('-') => {
            eprintln!("Unknown option: {arg}");
            eprintln!("Run `icebox help` for usage.");
            std::process::exit(1);
        }
        Some(path) => {
            let workspace = resolve_workspace(path)?;
            return run_tui(&workspace);
        }
        None => {}
    }

    let workspace = env::current_dir().context("failed to get current directory")?;
    run_tui(&workspace)
}

fn print_help() {
    println!("icebox — TUI Kanban Board + AI Sidebar");
    println!();
    println!("USAGE:");
    println!("  icebox                Launch the TUI kanban board (current directory)");
    println!("  icebox [path]         Launch the TUI kanban board at the given path");
    println!("  icebox init           Initialize .icebox/ workspace (current directory)");
    println!("  icebox init [path]    Initialize .icebox/ workspace at the given path");
    println!("  icebox login          Authenticate via OAuth (opens browser)");
    println!("  icebox logout         Clear saved credentials");
    println!("  icebox whoami         Show current authentication status");
    println!("  icebox help           Show this help message");
    println!();
    println!("AUTHENTICATION (recommended: API key):");
    println!("  export ANTHROPIC_API_KEY=sk-ant-api03-...");
    println!("  Get key: https://console.anthropic.com/settings/keys");
    println!();
    println!("ENVIRONMENT:");
    println!("  ANTHROPIC_API_KEY   API key (recommended, highest priority)");
    println!(
        "  ANTHROPIC_MODEL     Model name (default: auth-aware; OAuth uses {})",
        icebox_runtime::DEFAULT_OAUTH_MODEL
    );
    println!("  ICEBOX_CONFIG_HOME  Config directory (default: ~/.icebox)");
}

// ── Login ──

fn run_login() -> Result<()> {
    println!("Icebox OAuth Login (Claude.ai)");
    println!("──────────────────────────────");

    if let Ok(Some(token_set)) = icebox_runtime::load_oauth_credentials() {
        if !icebox_runtime::is_token_expired(&token_set) {
            println!("Already authenticated (icebox OAuth token).");
            if let Some(exp) = token_set.expires_at {
                let remaining = exp.saturating_sub(icebox_runtime::now_unix());
                let hours = remaining / 3600;
                println!("  Expires in: ~{hours} hours");
            }
            println!();
            println!("Run `icebox logout` first to re-authenticate.");
            return Ok(());
        }
        println!("Existing token expired, starting new login...");
    }

    let config = icebox_runtime::OAuthConfig::code_display();
    let pkce = icebox_runtime::generate_pkce_pair();
    let state = icebox_runtime::generate_state();

    let auth_url =
        icebox_runtime::build_code_display_authorize_url(&config, &pkce.challenge, &state);

    println!("\nOpening browser for authentication...\n");
    let _ = open_browser(&auth_url);
    println!("If the browser doesn't open, visit this URL manually:");
    println!("  {auth_url}\n");

    print!("Authorization code: ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut code_input = String::new();
    std::io::stdin()
        .read_line(&mut code_input)
        .context("failed to read authorization code")?;

    let code = code_input.trim();
    if code.is_empty() {
        anyhow::bail!("no authorization code provided");
    }

    println!("Exchanging for token...\n");

    let token_set = icebox_runtime::exchange_code_json(&config, code, &pkce.verifier, &state)?;

    println!("Login successful!");
    println!("  Token saved to ~/.icebox/credentials.json");
    if !token_set.scopes.is_empty() {
        println!("  Scopes: {}", token_set.scopes.join(", "));
    }
    if let Some(exp) = token_set.expires_at {
        let remaining = exp.saturating_sub(icebox_runtime::now_unix());
        let hours = remaining / 3600;
        println!("  Expires in: ~{hours} hours");
    }
    println!();
    println!("Run `icebox` to start the kanban board.");

    Ok(())
}

fn run_logout() -> Result<()> {
    icebox_runtime::clear_oauth_credentials().context("failed to clear credentials")?;
    println!("Logged out. OAuth credentials cleared.");
    Ok(())
}

fn run_whoami() -> Result<()> {
    match icebox_runtime::AuthSource::resolve() {
        Ok(auth) => {
            print_auth_status(&auth);

            if let Ok(Some(account)) = icebox_runtime::load_active_claude_code_account() {
                println!("  Source: Claude Code ({})", account.source);
                if let Some(subscription_type) = &account.credentials.subscription_type {
                    println!("  Plan: {subscription_type}");
                }
                if let Some(rate_limit_tier) = &account.credentials.rate_limit_tier {
                    println!("  Rate Limit Tier: {rate_limit_tier}");
                }
                if account.credentials.scopes.is_empty() {
                    println!("  Scopes: (none)");
                } else {
                    println!("  Scopes: {}", account.credentials.scopes.join(", "));
                }
                let expires_at = account.credentials.expires_at;
                let expires_secs = if expires_at > 1_000_000_000_000 {
                    expires_at / 1000
                } else {
                    expires_at
                };
                let remaining = expires_secs.saturating_sub(icebox_runtime::now_unix());
                let hours = remaining / 3600;
                let mins = (remaining % 3600) / 60;
                println!("  Expires in: {hours}h {mins}m");
            } else if let Ok(Some(token_set)) = icebox_runtime::load_oauth_credentials() {
                if token_set.scopes.is_empty() {
                    println!("  Scopes: (none)");
                } else {
                    println!("  Scopes: {}", token_set.scopes.join(", "));
                }
                if !token_set
                    .scopes
                    .iter()
                    .any(|scope| scope.contains("inference"))
                {
                    println!("  WARNING: missing 'user:inference' — API calls will 401");
                }
                if icebox_runtime::is_token_expired(&token_set) {
                    println!("  Status: EXPIRED — run `icebox login`");
                } else if let Some(expires_at) = token_set.expires_at {
                    let remaining = expires_at.saturating_sub(icebox_runtime::now_unix());
                    let hours = remaining / 3600;
                    let mins = (remaining % 3600) / 60;
                    println!("  Expires in: {hours}h {mins}m");
                }
            }
        }
        Err(e) => {
            println!("Not authenticated: {e}");
            println!();
            println!("Set ANTHROPIC_API_KEY or run `icebox login`.");
        }
    }
    Ok(())
}

fn print_auth_status(auth: &icebox_runtime::AuthSource) {
    match auth {
        icebox_runtime::AuthSource::ApiKey(key) => {
            let masked = mask_key(key);
            println!("  Auth: API Key ({masked})");
        }
        icebox_runtime::AuthSource::BearerToken(_) => {
            println!("  Auth: OAuth Bearer Token");
        }
        icebox_runtime::AuthSource::ApiKeyAndBearer { api_key, .. } => {
            let masked = mask_key(api_key);
            println!("  Auth: API Key ({masked}) + Bearer Token");
        }
        icebox_runtime::AuthSource::None => {
            println!("  Auth: None");
        }
    }
}

fn mask_key(key: &str) -> String {
    let len = key.len();
    if len <= 8 {
        return "***".to_string();
    }
    let prefix: String = key.chars().take(4).collect();
    let suffix: String = key.chars().skip(len - 4).collect();
    format!("{prefix}...{suffix}")
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .context("failed to open browser")?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .context("failed to open browser")?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/C", "start", url])
            .spawn()
            .context("failed to open browser")?;
    }
    Ok(())
}

// ── Test API ──

fn run_test_api() -> Result<()> {
    println!("Icebox API Test");
    println!("───────────────");

    let auth = icebox_runtime::AuthSource::resolve().context("failed to resolve auth")?;

    // Display auth info
    let is_oauth = matches!(&auth, icebox_runtime::AuthSource::BearerToken(_));
    print_auth_status(&auth);

    let base_url = match env::var("ANTHROPIC_BASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => "https://api.anthropic.com".to_string(),
    };

    let model = match env::var("ANTHROPIC_MODEL") {
        Ok(m) if !m.is_empty() => m,
        _ => icebox_runtime::default_model_for_auth(is_oauth).to_string(),
    };

    println!("  URL: {base_url}/v1/messages");
    println!("  Model: {model}");
    if is_oauth {
        println!("  Mode: OAuth Bearer (opencode-claude-auth pattern)");
    }
    println!("  Retry: up to 5 attempts with exponential backoff");
    println!();
    // Print curl equivalent for debugging
    let token_display = match &auth {
        icebox_runtime::AuthSource::BearerToken(token) => mask_secret(token),
        icebox_runtime::AuthSource::ApiKey(api_key) => mask_secret(api_key),
        icebox_runtime::AuthSource::ApiKeyAndBearer { bearer_token, .. } => {
            mask_secret(bearer_token)
        }
        icebox_runtime::AuthSource::None => "(none)".to_string(),
    };
    eprintln!();
    eprintln!("  Debug: curl equivalent (paste in terminal to test):");
    if is_oauth {
        eprintln!("  curl -s 'https://api.anthropic.com/v1/messages?beta=true' \\");
    } else {
        eprintln!("  curl -s https://api.anthropic.com/v1/messages \\");
    }
    eprintln!("    -H 'anthropic-version: 2023-06-01' \\");
    eprintln!("    -H 'content-type: application/json' \\");
    if is_oauth {
        eprintln!("    -H 'authorization: Bearer {token_display}' \\");
        eprintln!(
            "    -H 'anthropic-beta: {}' \\",
            icebox_api::oauth_transform::REQUIRED_BETAS.join(",")
        );
        eprintln!("    -H 'user-agent: claude-cli/2.1.91 (external, cli)' \\");
    } else {
        let key = match &auth {
            icebox_runtime::AuthSource::ApiKey(k) => k.as_str(),
            _ => "",
        };
        eprintln!("    -H 'x-api-key: {key}' \\");
        eprintln!("    -H 'anthropic-beta: interleaved-thinking-2025-05-14' \\");
    }
    eprintln!(
        "    -d '{{\"model\":\"{model}\",\"max_tokens\":64,\"messages\":[{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"Hi\"}}]}}]}}'"
    );
    eprintln!();
    println!("  Sending test request...");

    // Create AnthropicClient from resolved auth (gets retry + Retry-After for free)
    let client = match auth {
        icebox_runtime::AuthSource::ApiKey(key) => icebox_api::AnthropicClient::new(key),
        icebox_runtime::AuthSource::BearerToken(token) => {
            icebox_api::AnthropicClient::from_bearer(token)
        }
        icebox_runtime::AuthSource::ApiKeyAndBearer {
            api_key,
            bearer_token,
        } => icebox_api::AnthropicClient::from_combined(api_key, bearer_token),
        icebox_runtime::AuthSource::None => anyhow::bail!("No auth configured"),
    };
    let client = client.with_base_url(base_url);

    let request = icebox_api::types::MessageRequest {
        model: model.clone(),
        max_tokens: 64,
        messages: vec![icebox_api::types::InputMessage::user_text(
            "Say hello in 3 words.",
        )],
        system: None,
        tools: Some(vec![]),
        tool_choice: None,
        stream: false,
    };

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    match rt.block_on(client.send_message(&request)) {
        Ok(response) => {
            println!();
            // Extract text from response
            for block in &response.content {
                if let icebox_api::types::OutputContentBlock::Text { text } = block {
                    println!("  Response: {text}");
                }
            }
            println!(
                "  Tokens: {} input, {} output",
                response.usage.input_tokens, response.usage.output_tokens
            );
            println!();
            println!("  ✓ API is working!");
        }
        Err(e) => {
            println!();
            println!("  ✗ API test failed: {e}");

            match e.status_code() {
                Some(401) => {
                    println!();
                    println!("  Run: icebox logout && icebox login");
                }
                Some(403) => {
                    println!();
                    println!("  Token may lack required scopes. Run: icebox whoami");
                }
                Some(429) => {
                    println!();
                    println!("  Rate limited even after retries. Wait a few minutes.");
                    if is_oauth && !model.contains("haiku") {
                        println!(
                            "  Claude Code OAuth on this machine currently succeeds with {}.",
                            icebox_runtime::DEFAULT_OAUTH_MODEL
                        );
                        println!(
                            "  Try: ANTHROPIC_MODEL={} icebox",
                            icebox_runtime::DEFAULT_OAUTH_MODEL
                        );
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

// ── Init ──

fn run_init(args: &[String]) -> Result<()> {
    let workspace = match args.get(2) {
        Some(path) => {
            let p = PathBuf::from(path);
            if p.is_absolute() {
                p
            } else {
                env::current_dir()
                    .context("failed to get current directory")?
                    .join(p)
            }
        }
        None => env::current_dir().context("failed to get current directory")?,
    };

    if !workspace.is_dir() {
        fs::create_dir_all(&workspace)
            .with_context(|| format!("failed to create directory: {}", workspace.display()))?;
    }

    let fresh = icebox_task::init_workspace(&workspace)?;
    let icebox_dir = workspace.join(".icebox");

    if fresh {
        println!("Initialized icebox workspace at {}", icebox_dir.display());
    } else {
        println!("Icebox workspace already exists at {}", icebox_dir.display());
    }

    Ok(())
}

fn resolve_workspace(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    let resolved = if p.is_absolute() {
        p
    } else {
        env::current_dir()
            .context("failed to get current directory")?
            .join(p)
    };

    if !resolved.is_dir() {
        anyhow::bail!("not a directory: {}", resolved.display());
    }

    Ok(resolved)
}

// ── TUI ──

fn run_tui(workspace: &std::path::Path) -> Result<()> {
    let store = TaskStore::open(workspace)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original_hook(info);
    }));

    let mut app = App::new(store, workspace)?;
    setup_ai_runtime(&mut app, workspace);

    let result = app.run(&mut terminal);
    restore_terminal()?;
    result
}

fn setup_ai_runtime(app: &mut App, workspace: &std::path::Path) {
    let resolved_auth = match icebox_runtime::AuthSource::resolve() {
        Ok(auth) => auth,
        Err(_) => return,
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
        icebox_runtime::AuthSource::None => return,
    };

    let store = match icebox_task::store::TaskStore::open(workspace) {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(_) => return,
    };

    // Model priority: ANTHROPIC_MODEL env > saved config > auth-aware default
    let model = match env::var("ANTHROPIC_MODEL") {
        Ok(m) if !m.is_empty() => {
            icebox_runtime::resolve_model(&m).map_or(m.clone(), |info| info.id.to_string())
        }
        _ => icebox_runtime::IceboxConfig::saved_model()
            .and_then(|m| icebox_runtime::resolve_model(&m).map(|info| info.id.to_string()))
            .unwrap_or_else(|| icebox_runtime::default_model_for_auth(is_oauth).to_string()),
    };
    app.current_model = model.clone();

    let tools = match icebox_tools::IceboxToolExecutor::new(workspace.to_path_buf(), store) {
        Ok(t) => Arc::new(t),
        Err(_) => return,
    };

    let (tui_tx, tui_rx) = tokio::sync::mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) =
        tokio::sync::mpsc::unbounded_channel::<icebox_runtime::RuntimeCommand>();
    let (approval_tx, mut approval_rx) =
        tokio::sync::mpsc::unbounded_channel::<icebox_runtime::ToolApproval>();

    let rt_client = client;
    let rt_model = model;
    let rt_tx = tui_tx.clone();
    let rt_tools = tools;
    let rt_workspace = workspace.to_path_buf();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return,
        };

        rt.block_on(async move {
            let mut runtime = icebox_runtime::ConversationRuntime::new(rt_client, rt_model);
            runtime.set_system_prompt(
                "You are an AI assistant integrated into the Icebox kanban board. \
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
                 - update_task: Update an existing task (title, priority, tags, swimlane, start_date, due_date, body)\n\
                 - move_task: Move a task to another column\n\
                 - save_memory / list_memories / delete_memory: Persistent memory\n\
                 \n\
                 CRITICAL: Use ONLY these exact tool names. NEVER add prefixes like 'mcp_' or 'icebox_'. \
                 For example, use 'update_task' not 'mcp_update_task'.",
            );

            // Per-session storage: maps session key → Session
            let mut sessions =
                std::collections::HashMap::<String, icebox_runtime::Session>::new();
            // Track which session is currently loaded in the runtime
            let mut current_session_key: Option<String> = None;
            let mut auto_approve = false;

            loop {
                let Some(cmd) = cmd_rx.recv().await else {
                    break;
                };

                match cmd {
                    icebox_runtime::RuntimeCommand::SendMessage { session_id, input } => {
                        let key = session_id
                            .clone()
                            .unwrap_or_else(|| icebox_runtime::GLOBAL_SESSION_KEY.to_string());

                        // Swap out current session if it's a different one
                        if current_session_key.as_ref() != Some(&key) {
                            // Save current session back to cache
                            if let Some(old_key) = current_session_key.take() {
                                let old_session = runtime.swap_session(
                                    icebox_runtime::Session::new(),
                                );
                                sessions.insert(old_key, old_session);
                            }

                            // Load target session from cache or disk
                            let target = sessions.remove(&key).unwrap_or_else(|| {
                                let path =
                                    icebox_runtime::session_path(&rt_workspace, &key);
                                icebox_runtime::Session::load_from_path(&path)
                                    .unwrap_or_default()
                            });
                            runtime.swap_session(target);
                            current_session_key = Some(key.clone());
                        }

                        // Emit session context so TUI knows where to route events
                        let _ = rt_tx.send(icebox_runtime::AiEvent::SessionContext {
                            session_id,
                        });

                        // Run the conversation turn
                        if let Err(e) = runtime
                            .run_turn(
                                &input,
                                rt_tools.as_ref(),
                                &rt_tx,
                                &mut approval_rx,
                                &mut auto_approve,
                            )
                            .await
                        {
                            let _ = rt_tx
                                .send(icebox_runtime::AiEvent::Error(format!("{e}")));
                        }

                        // Save session to disk after turn completes
                        let session_snapshot = runtime.session().clone();
                        let path = icebox_runtime::session_path(&rt_workspace, &key);
                        if let Err(e) = session_snapshot.save_to_path(&path) {
                            let _ = rt_tx.send(icebox_runtime::AiEvent::Error(
                                format!("Failed to save session: {e}"),
                            ));
                        }
                    }
                    icebox_runtime::RuntimeCommand::SwitchModel(new_model) => {
                        runtime.set_model(&new_model);
                    }
                    icebox_runtime::RuntimeCommand::ClearSession { session_id } => {
                        let key = session_id
                            .unwrap_or_else(|| icebox_runtime::GLOBAL_SESSION_KEY.to_string());

                        // Remove from cache
                        sessions.remove(&key);

                        // If it's the current session, reset it
                        if current_session_key.as_ref() == Some(&key) {
                            runtime.swap_session(icebox_runtime::Session::new());
                        }

                        // Delete from disk
                        let path = icebox_runtime::session_path(&rt_workspace, &key);
                        let _ = std::fs::remove_file(&path);
                    }
                    icebox_runtime::RuntimeCommand::CompactSession { session_id } => {
                        let key = session_id
                            .unwrap_or_else(|| icebox_runtime::GLOBAL_SESSION_KEY.to_string());

                        // Swap to target session if needed
                        if current_session_key.as_ref() != Some(&key) {
                            if let Some(old_key) = current_session_key.take() {
                                let old_session =
                                    runtime.swap_session(icebox_runtime::Session::new());
                                sessions.insert(old_key, old_session);
                            }
                            let target = sessions.remove(&key).unwrap_or_else(|| {
                                let path = icebox_runtime::session_path(&rt_workspace, &key);
                                icebox_runtime::Session::load_from_path(&path)
                                    .unwrap_or_default()
                            });
                            runtime.swap_session(target);
                            current_session_key = Some(key.clone());
                        }

                        runtime.compact();

                        // Save compacted session
                        let session_snapshot = runtime.session().clone();
                        let path = icebox_runtime::session_path(&rt_workspace, &key);
                        let _ = session_snapshot.save_to_path(&path);
                    }
                }
            }
        });
    });

    let sender: icebox_tui::app::AiSender =
        Box::new(move |cmd: icebox_runtime::RuntimeCommand| {
            let _ = cmd_tx.send(cmd);
        });

    app.set_ai_channel(tui_rx, sender, approval_tx);
}

fn mask_secret(secret: &str) -> String {
    let char_count = secret.chars().count();
    if char_count <= 8 {
        return "***".to_string();
    }
    let prefix: String = secret.chars().take(8).collect();
    let suffix: String = secret
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

fn restore_terminal() -> Result<()> {
    let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen);
    let _ = crossterm::execute!(io::stdout(), crossterm::event::DisableMouseCapture);
    print!("\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l");
    let _ = io::Write::flush(&mut io::stdout());
    Ok(())
}
