use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::usage::TokenUsage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub blocks: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: 1,
            messages: Vec::new(),
        }
    }

    pub fn push_user_text(&mut self, text: impl Into<String>) {
        self.messages.push(ConversationMessage {
            role: MessageRole::User,
            blocks: vec![ContentBlock::Text { text: text.into() }],
            usage: None,
        });
    }

    pub fn push_assistant(&mut self, blocks: Vec<ContentBlock>, usage: Option<TokenUsage>) {
        self.messages.push(ConversationMessage {
            role: MessageRole::Assistant,
            blocks,
            usage,
        });
    }

    pub fn push_tool_result(
        &mut self,
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    ) {
        self.messages.push(ConversationMessage {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            }],
            usage: None,
        });
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("failed to serialize session")?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create session dir: {}", parent.display()))?;
        }
        fs::write(path, &json)
            .with_context(|| format!("failed to write session: {}", path.display()))?;
        Ok(())
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read session: {}", path.display()))?;
        let session: Self =
            serde_json::from_str(&content).context("failed to parse session JSON")?;
        Ok(session)
    }

    #[must_use]
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn estimated_tokens(&self) -> u32 {
        let mut total: u32 = 0;
        for msg in &self.messages {
            for block in &msg.blocks {
                let text_len = match block {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ToolUse { input, .. } => input.len(),
                    ContentBlock::ToolResult { output, .. } => output.len(),
                };
                // Rough estimate: ~4 chars per token
                total += (text_len as u32) / 4 + 1;
            }
        }
        total
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Session key used for the global (non-task-specific) chat session.
pub const GLOBAL_SESSION_KEY: &str = "__global__";

/// Returns the path for a session file: `{workspace}/.icebox/sessions/{session_id}.json`
#[must_use]
pub fn session_path(workspace: &Path, session_id: &str) -> PathBuf {
    workspace
        .join(".icebox")
        .join("sessions")
        .join(format!("{session_id}.json"))
}
