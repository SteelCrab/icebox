/// Slash command definitions for the icebox kanban board.

#[derive(Debug, Clone)]
pub enum SlashCommand {
    Help,
    Status,
    Cost,
    Clear,
    Compact,
    New { title: Option<String> },
    Move { column: Option<String> },
    Delete { id: Option<String> },
    Search { query: Option<String> },
    Model { model: Option<String> },
    Remember { text: Option<String> },
    Memory,
    Resume { session: Option<String> },
    Export,
    Diff,
    Notion { action: Option<String> },
    Login,
    Logout,
    Unknown(String),
}

pub struct SlashCommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub argument_hint: Option<&'static str>,
    pub category: CommandCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    Board,
    Ai,
    Auth,
    Session,
}

impl CommandCategory {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Board => "Board",
            Self::Ai => "AI",
            Self::Auth => "Auth",
            Self::Session => "Session",
        }
    }
}

pub const COMMANDS: &[SlashCommandSpec] = &[
    // Board
    SlashCommandSpec {
        name: "new",
        aliases: &["n"],
        summary: "Create a new task on the board",
        argument_hint: Some("<title>"),
        category: CommandCategory::Board,
    },
    SlashCommandSpec {
        name: "move",
        aliases: &["mv"],
        summary: "Move selected task to a column",
        argument_hint: Some("<icebox|emergency|inprogress|testing|complete>"),
        category: CommandCategory::Board,
    },
    SlashCommandSpec {
        name: "delete",
        aliases: &["del", "rm"],
        summary: "Delete a task by ID prefix",
        argument_hint: Some("<task-id>"),
        category: CommandCategory::Board,
    },
    SlashCommandSpec {
        name: "search",
        aliases: &["find", "s"],
        summary: "Search tasks by title keyword",
        argument_hint: Some("<query>"),
        category: CommandCategory::Board,
    },
    SlashCommandSpec {
        name: "export",
        aliases: &[],
        summary: "Export board summary as markdown",
        argument_hint: None,
        category: CommandCategory::Board,
    },
    SlashCommandSpec {
        name: "diff",
        aliases: &[],
        summary: "Show git diff of task changes",
        argument_hint: None,
        category: CommandCategory::Board,
    },
    SlashCommandSpec {
        name: "notion",
        aliases: &[],
        summary: "Sync tasks to Notion database",
        argument_hint: Some("[push|status|reset]"),
        category: CommandCategory::Board,
    },
    // AI
    SlashCommandSpec {
        name: "help",
        aliases: &["h", "?"],
        summary: "Show available slash commands",
        argument_hint: None,
        category: CommandCategory::Ai,
    },
    SlashCommandSpec {
        name: "status",
        aliases: &[],
        summary: "Show session status, token count, AI connection",
        argument_hint: None,
        category: CommandCategory::Ai,
    },
    SlashCommandSpec {
        name: "cost",
        aliases: &[],
        summary: "Show cumulative token usage and cost estimate",
        argument_hint: None,
        category: CommandCategory::Ai,
    },
    SlashCommandSpec {
        name: "clear",
        aliases: &[],
        summary: "Clear AI conversation history",
        argument_hint: None,
        category: CommandCategory::Ai,
    },
    SlashCommandSpec {
        name: "compact",
        aliases: &[],
        summary: "Compact conversation (summarize old messages)",
        argument_hint: None,
        category: CommandCategory::Ai,
    },
    SlashCommandSpec {
        name: "model",
        aliases: &[],
        summary: "Show or switch the AI model",
        argument_hint: Some("[model-name]"),
        category: CommandCategory::Ai,
    },
    // Auth
    SlashCommandSpec {
        name: "login",
        aliases: &[],
        summary: "Authenticate via OAuth (opens browser)",
        argument_hint: None,
        category: CommandCategory::Auth,
    },
    SlashCommandSpec {
        name: "logout",
        aliases: &[],
        summary: "Clear saved OAuth credentials",
        argument_hint: None,
        category: CommandCategory::Auth,
    },
    SlashCommandSpec {
        name: "remember",
        aliases: &["rem"],
        summary: "Save a memory for AI context",
        argument_hint: Some("<text>"),
        category: CommandCategory::Ai,
    },
    SlashCommandSpec {
        name: "memory",
        aliases: &["mem"],
        summary: "Switch to memory management view",
        argument_hint: None,
        category: CommandCategory::Ai,
    },
    // Session
    SlashCommandSpec {
        name: "resume",
        aliases: &[],
        summary: "Resume a previous AI session",
        argument_hint: Some("[session-id]"),
        category: CommandCategory::Session,
    },
];

impl SlashCommand {
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let without_slash = &trimmed[1..];
        let mut parts = without_slash.splitn(2, ' ');
        let cmd = parts.next()?;
        let arg = parts.next().map(|s| s.trim().to_string());

        let command = match cmd {
            "help" | "h" | "?" => Self::Help,
            "status" => Self::Status,
            "cost" => Self::Cost,
            "clear" => Self::Clear,
            "compact" => Self::Compact,
            "new" | "n" => Self::New { title: arg },
            "move" | "mv" => Self::Move { column: arg },
            "delete" | "del" | "rm" => Self::Delete { id: arg },
            "search" | "find" | "s" => Self::Search { query: arg },
            "model" => Self::Model { model: arg },
            "remember" | "rem" => Self::Remember { text: arg },
            "memory" | "mem" => Self::Memory,
            "resume" => Self::Resume { session: arg },
            "export" => Self::Export,
            "diff" => Self::Diff,
            "notion" => Self::Notion { action: arg },
            "login" => Self::Login,
            "logout" => Self::Logout,
            other => Self::Unknown(other.to_string()),
        };

        Some(command)
    }
}

/// Filter commands matching a prefix (e.g. "/he" matches "/help").
/// Returns matching specs sorted by category.
pub fn filter_commands(prefix: &str) -> Vec<&'static SlashCommandSpec> {
    let query = prefix.strip_prefix('/').unwrap_or(prefix).to_lowercase();

    if query.is_empty() {
        // Show all commands when just "/" is typed
        return COMMANDS.iter().collect();
    }

    COMMANDS
        .iter()
        .filter(|spec| {
            spec.name.starts_with(&query)
                || spec.aliases.iter().any(|alias| alias.starts_with(&query))
        })
        .collect()
}

/// Autocomplete: find the single best match for a prefix.
/// Returns the full command name if exactly one match.
pub fn autocomplete(prefix: &str) -> Option<&'static str> {
    let matches = filter_commands(prefix);
    if matches.len() == 1 {
        Some(matches[0].name)
    } else {
        None
    }
}

/// Render a formatted help string showing all commands grouped by category.
pub fn render_help() -> String {
    let mut lines = vec![String::from("Commands:")];

    let categories = [
        CommandCategory::Board,
        CommandCategory::Ai,
        CommandCategory::Auth,
        CommandCategory::Session,
    ];

    for cat in &categories {
        lines.push(String::new());
        lines.push(format!("  [{}]", cat.label()));

        for spec in COMMANDS {
            if spec.category != *cat {
                continue;
            }
            let name = match spec.argument_hint {
                Some(hint) => format!("/{} {hint}", spec.name),
                None => format!("/{}", spec.name),
            };
            let aliases = if spec.aliases.is_empty() {
                String::new()
            } else {
                let a: Vec<String> = spec.aliases.iter().map(|a| format!("/{a}")).collect();
                format!(" ({})", a.join(", "))
            };
            lines.push(format!("    {name:<35} {}{aliases}", spec.summary));
        }
    }

    lines.join("\n")
}
