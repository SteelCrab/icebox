use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

impl TokenUsage {
    #[must_use]
    pub const fn total_tokens(&self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
    }
}

// ── Model registry ──

pub struct ModelInfo {
    pub alias: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub id: &'static str,
    pub max_tokens: u32,
    pub context_window: &'static str,
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: f64,
    pub cache_read_per_million: f64,
    pub is_default: bool,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        alias: "opus",
        display_name: "Opus (1M context)",
        description: "Opus 4.6 with 1M context · Most capable for complex work",
        id: "claude-opus-4-6",
        max_tokens: 128_000,
        context_window: "1M",
        input_per_million: 15.0,
        output_per_million: 75.0,
        cache_write_per_million: 18.75,
        cache_read_per_million: 1.5,
        is_default: true,
    },
    ModelInfo {
        alias: "sonnet",
        display_name: "Sonnet",
        description: "Sonnet 4.6 · Best for everyday tasks",
        id: "claude-sonnet-4-6",
        max_tokens: 64_000,
        context_window: "200K",
        input_per_million: 15.0,
        output_per_million: 75.0,
        cache_write_per_million: 18.75,
        cache_read_per_million: 1.5,
        is_default: false,
    },
    ModelInfo {
        alias: "haiku",
        display_name: "Haiku",
        description: "Haiku 4.5 · Fastest for quick answers",
        id: "claude-haiku-4-5-20251001",
        max_tokens: 64_000,
        context_window: "200K",
        input_per_million: 1.0,
        output_per_million: 5.0,
        cache_write_per_million: 1.25,
        cache_read_per_million: 0.1,
        is_default: false,
    },
];

/// Effort levels for model inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    Low,
    Medium,
    High,
    Max,
}

impl Effort {
    pub const ALL: [Effort; 4] = [Self::Low, Self::Medium, Self::High, Self::Max];

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Max => "Max",
        }
    }

    #[must_use]
    pub fn indicator(&self) -> &'static str {
        match self {
            Self::Low => "○ ○ ○ ○",
            Self::Medium => "● ○ ○ ○",
            Self::High => "● ● ● ○",
            Self::Max => "● ● ● ●",
        }
    }

    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Max,
            Self::Max => Self::Max,
        }
    }

    #[must_use]
    pub fn prev(&self) -> Self {
        match self {
            Self::Low => Self::Low,
            Self::Medium => Self::Low,
            Self::High => Self::Medium,
            Self::Max => Self::High,
        }
    }
}

pub const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
pub const DEFAULT_OAUTH_MODEL: &str = "claude-haiku-4-5-20251001";

#[must_use]
pub const fn default_model_for_auth(is_oauth: bool) -> &'static str {
    if is_oauth {
        DEFAULT_OAUTH_MODEL
    } else {
        DEFAULT_MODEL
    }
}

/// Resolve an alias ("opus", "sonnet", "haiku") or full model ID to a ModelInfo.
pub fn resolve_model(name: &str) -> Option<&'static ModelInfo> {
    let lower = name.to_lowercase();
    MODELS
        .iter()
        .find(|m| m.alias == lower || m.id == lower || lower.contains(m.alias))
}

/// Get max_tokens for a model (by alias or ID). Falls back to 8192.
pub fn max_tokens_for_model(model: &str) -> u32 {
    resolve_model(model).map_or(8192, |m| m.max_tokens)
}

/// Format the model list for display.
pub fn format_model_list(current_model: &str) -> String {
    let mut lines = vec![String::from("Available models:")];
    for m in MODELS {
        let marker = if current_model.contains(m.alias) || current_model == m.id {
            "→"
        } else {
            " "
        };
        lines.push(format!(
            "  {marker} {:<8} {:<28} {:>6}K  ${:.0}→${:.0}/M",
            m.alias,
            m.id,
            m.max_tokens / 1000,
            m.input_per_million,
            m.output_per_million,
        ));
    }
    lines.join("\n")
}

// ── Pricing ──

#[derive(Debug)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: f64,
    pub cache_read_per_million: f64,
}

impl ModelPricing {
    #[must_use]
    pub fn estimate_cost(&self, usage: &TokenUsage) -> f64 {
        let input = f64::from(usage.input_tokens) * self.input_per_million / 1_000_000.0;
        let output = f64::from(usage.output_tokens) * self.output_per_million / 1_000_000.0;
        let cache_write = f64::from(usage.cache_creation_input_tokens)
            * self.cache_write_per_million
            / 1_000_000.0;
        let cache_read =
            f64::from(usage.cache_read_input_tokens) * self.cache_read_per_million / 1_000_000.0;
        input + output + cache_write + cache_read
    }
}

pub fn pricing_for_model(model: &str) -> ModelPricing {
    match resolve_model(model) {
        Some(m) => ModelPricing {
            input_per_million: m.input_per_million,
            output_per_million: m.output_per_million,
            cache_write_per_million: m.cache_write_per_million,
            cache_read_per_million: m.cache_read_per_million,
        },
        None => ModelPricing {
            input_per_million: 15.0,
            output_per_million: 75.0,
            cache_write_per_million: 18.75,
            cache_read_per_million: 1.5,
        },
    }
}

#[must_use]
pub fn format_usd(amount: f64) -> String {
    if amount < 0.01 {
        format!("${amount:.4}")
    } else {
        format!("${amount:.2}")
    }
}

#[derive(Debug, Default)]
pub struct UsageTracker {
    pub cumulative: TokenUsage,
    pub latest_turn: TokenUsage,
    pub turn_count: u32,
}

impl UsageTracker {
    pub fn record_turn(&mut self, usage: TokenUsage) {
        self.cumulative.accumulate(&usage);
        self.latest_turn = usage;
        self.turn_count += 1;
    }

    #[must_use]
    pub fn cost_summary(&self, model: &str) -> String {
        let pricing = pricing_for_model(model);
        let cost = pricing.estimate_cost(&self.cumulative);
        format!(
            "Turns: {} | Tokens: {} in / {} out | Cost: {}",
            self.turn_count,
            self.cumulative.input_tokens,
            self.cumulative.output_tokens,
            format_usd(cost),
        )
    }
}
