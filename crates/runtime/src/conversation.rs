use anyhow::Result;
use tokio::sync::mpsc;

use icebox_api::types::{
    ContentBlockDelta, InputContentBlock, InputMessage, MessageRequest, OutputContentBlock,
    StreamEvent, ToolDefinition, Usage,
};
use icebox_api::{AnthropicClient, MessageStream};

use crate::session::{ContentBlock, Session};
use crate::usage::TokenUsage;

const MAX_TOOL_ITERATIONS: usize = 25;
const AUTO_COMPACT_THRESHOLD: u32 = 200_000;

/// Commands sent from the TUI to the conversation runtime
#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    /// Run a conversation turn for a specific session
    SendMessage {
        /// None = global/bottom chat, Some(task_id) = task-specific session
        session_id: Option<String>,
        input: String,
    },
    /// Switch the active model
    SwitchModel(String),
    /// Clear a session (runtime cache + disk)
    ClearSession { session_id: Option<String> },
    /// Compact a session (summarize old messages, keep recent)
    CompactSession { session_id: Option<String> },
}

/// User's response to a tool approval request
#[derive(Debug, Clone)]
pub enum ToolApproval {
    /// Approve this single tool call
    Yes,
    /// Approve all tool calls for this session
    AlwaysYes,
    /// Deny this tool call
    No,
}

/// Events sent from the conversation runtime to the TUI
#[derive(Debug, Clone)]
pub enum AiEvent {
    /// Identifies which session the following events belong to
    SessionContext {
        session_id: Option<String>,
    },
    TextDelta(String),
    /// Ask user for tool execution approval
    ToolApprovalRequest {
        name: String,
        input: String,
    },
    ToolCallStart {
        name: String,
        input: String,
    },
    ToolCallEnd {
        name: String,
        output: String,
        is_error: bool,
    },
    Usage(TokenUsage),
    TurnComplete,
    Error(String),
}

/// Trait for tool execution
pub trait ToolExecutor: Send + Sync {
    fn execute(&self, tool_name: &str, input: &str) -> Result<String>;

    fn tool_definitions(&self) -> Vec<ToolDefinition>;
}

pub struct ConversationRuntime {
    client: AnthropicClient,
    session: Session,
    model: String,
    max_tokens: u32,
    system_prompt: Option<String>,
}

impl ConversationRuntime {
    #[must_use]
    pub fn new(client: AnthropicClient, model: String) -> Self {
        Self {
            client,
            session: Session::new(),
            model,
            max_tokens: 8192,
            system_prompt: None,
        }
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        let model = model.into();
        self.max_tokens = crate::usage::max_tokens_for_model(&model);
        self.model = model;
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Swap the current session with a new one, returning the old session.
    pub fn swap_session(&mut self, session: Session) -> Session {
        std::mem::replace(&mut self.session, session)
    }

    /// Run a single conversation turn with streaming.
    /// Sends events via the `tx` channel for the TUI to display.
    /// Uses `approval_rx` to get user approval before executing tools.
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        tools: &dyn ToolExecutor,
        tx: &mpsc::UnboundedSender<AiEvent>,
        approval_rx: &mut mpsc::UnboundedReceiver<ToolApproval>,
        auto_approve: &mut bool,
    ) -> Result<TokenUsage> {
        self.session.push_user_text(user_input);

        let mut cumulative_usage = TokenUsage::default();

        for _iteration in 0..MAX_TOOL_ITERATIONS {
            let request = self.build_request(tools);
            let mut stream = self
                .client
                .stream_message(&request)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            let (blocks, usage) = self.process_stream(&mut stream, tx).await?;

            let api_usage = TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
            };
            cumulative_usage.accumulate(&api_usage);

            // Extract tool uses from blocks
            let tool_uses: Vec<(String, String, String)> = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            self.session.push_assistant(blocks, Some(api_usage));

            if tool_uses.is_empty() {
                break;
            }

            // Execute each tool (with approval)
            for (tool_id, tool_name_raw, tool_input) in &tool_uses {
                // Strip hallucinated prefixes for display
                let tool_name = tool_name_raw
                    .strip_prefix("mcp_")
                    .or_else(|| tool_name_raw.strip_prefix("icebox_"))
                    .unwrap_or(tool_name_raw)
                    .to_string();
                let tool_name = &tool_name;
                // Request approval unless auto-approved
                let approved = if *auto_approve {
                    true
                } else {
                    let _ = tx.send(AiEvent::ToolApprovalRequest {
                        name: tool_name.clone(),
                        input: truncate(tool_input, 200),
                    });
                    // Wait for user response
                    match approval_rx.recv().await {
                        Some(ToolApproval::Yes) => true,
                        Some(ToolApproval::AlwaysYes) => {
                            *auto_approve = true;
                            true
                        }
                        Some(ToolApproval::No) | None => false,
                    }
                };

                if !approved {
                    let _ = tx.send(AiEvent::ToolCallEnd {
                        name: tool_name.clone(),
                        output: "Tool execution denied by user.".into(),
                        is_error: true,
                    });
                    self.session.push_tool_result(
                        tool_id.clone(),
                        tool_name.clone(),
                        "Tool execution denied by user.".into(),
                        true,
                    );
                    continue;
                }

                let _ = tx.send(AiEvent::ToolCallStart {
                    name: tool_name.clone(),
                    input: truncate(tool_input, 200),
                });

                let (output, is_error) = match tools.execute(tool_name, tool_input) {
                    Ok(out) => (out, false),
                    Err(e) => (format!("Error: {e}"), true),
                };

                let _ = tx.send(AiEvent::ToolCallEnd {
                    name: tool_name.clone(),
                    output: truncate(&output, 500),
                    is_error,
                });

                self.session
                    .push_tool_result(tool_id.clone(), tool_name.clone(), output, is_error);
            }
        }

        let _ = tx.send(AiEvent::Usage(cumulative_usage.clone()));
        let _ = tx.send(AiEvent::TurnComplete);

        // Auto-compact check
        if self.session.estimated_tokens() > AUTO_COMPACT_THRESHOLD {
            self.compact();
        }

        Ok(cumulative_usage)
    }

    fn build_request(&self, tools: &dyn ToolExecutor) -> MessageRequest {
        let messages = self.build_messages();
        let tool_defs = tools.tool_definitions();

        MessageRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages,
            system: self.system_prompt.clone(),
            // OAuth requires tools array to always be present (even if empty)
            tools: Some(tool_defs),
            tool_choice: None,
            stream: true,
        }
    }

    fn build_messages(&self) -> Vec<InputMessage> {
        let mut messages = Vec::new();

        for msg in &self.session.messages {
            match msg.role {
                crate::session::MessageRole::User => {
                    let content: Vec<InputContentBlock> = msg
                        .blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => {
                                Some(InputContentBlock::Text { text: text.clone() })
                            }
                            _ => None,
                        })
                        .collect();
                    if !content.is_empty() {
                        messages.push(InputMessage {
                            role: "user".to_string(),
                            content,
                        });
                    }
                }
                crate::session::MessageRole::Assistant => {
                    let content: Vec<InputContentBlock> = msg
                        .blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => {
                                Some(InputContentBlock::Text { text: text.clone() })
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                let value = serde_json::from_str(input)
                                    .unwrap_or_else(|_| serde_json::json!({}));
                                Some(InputContentBlock::ToolUse {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: value,
                                })
                            }
                            _ => None,
                        })
                        .collect();
                    if !content.is_empty() {
                        messages.push(InputMessage {
                            role: "assistant".to_string(),
                            content,
                        });
                    }
                }
                crate::session::MessageRole::Tool => {
                    let content: Vec<InputContentBlock> = msg
                        .blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::ToolResult {
                                tool_use_id,
                                output,
                                is_error,
                                ..
                            } => Some(InputContentBlock::ToolResult {
                                tool_use_id: tool_use_id.clone(),
                                content: vec![icebox_api::types::ToolResultContentBlock::Text {
                                    text: output.clone(),
                                }],
                                is_error: *is_error,
                            }),
                            _ => None,
                        })
                        .collect();
                    if !content.is_empty() {
                        messages.push(InputMessage {
                            role: "user".to_string(),
                            content,
                        });
                    }
                }
                crate::session::MessageRole::System => {}
            }
        }

        messages
    }

    async fn process_stream(
        &self,
        stream: &mut MessageStream,
        tx: &mpsc::UnboundedSender<AiEvent>,
    ) -> Result<(Vec<ContentBlock>, Usage)> {
        let mut blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut tool_json_parts: std::collections::HashMap<u32, (String, String, String)> =
            std::collections::HashMap::new();
        let mut usage = Usage::default();

        while let Some(event) = stream
            .next_event()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
        {
            match event {
                StreamEvent::ContentBlockStart(e) => match e.content_block {
                    OutputContentBlock::Text { text } => {
                        if !current_text.is_empty() {
                            blocks.push(ContentBlock::Text {
                                text: std::mem::take(&mut current_text),
                            });
                        }
                        current_text = text;
                    }
                    OutputContentBlock::ToolUse { id, name, .. } => {
                        tool_json_parts.insert(e.index, (id, name, String::new()));
                    }
                },
                StreamEvent::ContentBlockDelta(e) => match e.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        current_text.push_str(&text);
                        let _ = tx.send(AiEvent::TextDelta(text));
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some((_id, _name, json)) = tool_json_parts.get_mut(&e.index) {
                            json.push_str(&partial_json);
                        }
                    }
                },
                StreamEvent::ContentBlockStop(_) => {}
                StreamEvent::MessageDelta(e) => {
                    usage = e.usage;
                }
                StreamEvent::MessageStart(_) | StreamEvent::MessageStop(_) => {}
            }
        }

        if !current_text.is_empty() {
            blocks.push(ContentBlock::Text { text: current_text });
        }

        for (_index, (id, name, json)) in tool_json_parts {
            blocks.push(ContentBlock::ToolUse {
                id,
                name,
                input: json,
            });
        }

        Ok((blocks, usage))
    }

    pub fn compact(&mut self) {
        let preserve_recent = 4;
        let msg_count = self.session.messages.len();
        if msg_count <= preserve_recent {
            return;
        }

        let cutoff = msg_count - preserve_recent;
        let old_messages = &self.session.messages[..cutoff];

        let mut summary = String::from("[Conversation compacted. Previous context summary:]\n");
        let mut user_requests = Vec::new();
        for msg in old_messages {
            if msg.role == crate::session::MessageRole::User {
                for block in &msg.blocks {
                    if let ContentBlock::Text { text } = block {
                        let preview: String = text.chars().take(100).collect();
                        user_requests.push(preview);
                    }
                }
            }
        }
        if !user_requests.is_empty() {
            summary.push_str("User requests: ");
            for (i, req) in user_requests.iter().enumerate().rev().take(3) {
                summary.push_str(&format!("\n  {}: {req}", i + 1));
            }
            summary.push('\n');
        }

        let recent = self.session.messages[cutoff..].to_vec();
        self.session.messages.clear();
        self.session
            .messages
            .push(crate::session::ConversationMessage {
                role: crate::session::MessageRole::System,
                blocks: vec![ContentBlock::Text { text: summary }],
                usage: None,
            });
        self.session.messages.extend(recent);
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}
