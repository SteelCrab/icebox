use crate::board::BoardState;
use crate::layout::{self, AppLayout};
use crate::sidebar::{self, MessageRole, SidebarMessage, SidebarState};
use crate::theme;
use chrono::Utc;
use crossterm::event;
use icebox_runtime::{AiEvent, RuntimeCommand};
use icebox_task::model::{Column, Priority, Task};
use icebox_task::store::TaskStore;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;
use tokio::sync::mpsc;
use icebox_tools::notion::NotionPage;

/// Braille spinner frames for AI thinking animation (from openpista pattern).
pub const SPINNER_FRAMES: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];

fn spinner_char(tick: u16) -> char {
    SPINNER_FRAMES[(tick as usize / 3) % SPINNER_FRAMES.len()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Board,
    TaskDetail,
    EditTask,
    CreateTask,
    ConfirmDelete,
    SelectModel,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Board,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTarget {
    None,
    SidebarDetail,
    SidebarChat,
    BottomChat,
}

/// Callback type for sending commands to the AI runtime
pub type AiSender = Box<dyn Fn(RuntimeCommand) + Send + Sync>;

pub struct PendingToolApproval {
    pub tool_name: String,
    pub tool_input: String,
}

pub struct App {
    pub mode: AppMode,
    pub board: BoardState,
    pub sidebar: SidebarState,
    pub sidebar_focused: bool,
    pub store: TaskStore,
    pub should_quit: bool,
    pub column_rects: Vec<Rect>,
    pub status_message: Option<StatusMessage>,
    pub ai_rx: Option<mpsc::UnboundedReceiver<AiEvent>>,
    pub ai_sender: Option<AiSender>,
    pub ai_busy: bool,
    pub approval_tx: Option<mpsc::UnboundedSender<icebox_runtime::ToolApproval>>,
    pub pending_tool_approval: Option<PendingToolApproval>,
    pub workspace_name: String,
    pub workspace_path: std::path::PathBuf,
    pub git_branch: Option<String>,
    pub sidebar_rect: Option<Rect>,

    // Edit task state
    pub edit_title: String,
    pub edit_body: String,
    pub edit_field: EditField,

    // Bottom global chat
    pub bottom_chat: SidebarState,
    pub bottom_chat_open: bool,
    pub bottom_chat_focused: bool,
    pub bottom_chat_rect: Option<Rect>,
    pub bottom_chat_height: u16,

    // Mouse drag state
    pub drag_start_y: Option<u16>,
    pub drag_target: DragTarget,

    // Per-task session state
    pub sidebar_messages: HashMap<String, Vec<SidebarMessage>>,
    pub active_task_id: Option<String>,
    pub ai_target_session: Option<Option<String>>,

    // Model selection
    pub current_model: String,
    pub model_select_idx: usize,
    pub effort: icebox_runtime::Effort,

    // Tab & Memory
    pub active_tab: Tab,
    pub memory_store: Option<icebox_task::memory::MemoryStore>,
    pub memory_entries: Vec<icebox_task::memory::MemoryEntry>,
    pub memory_selected: usize,
    pub memory_scroll: u16,

    // Spinner animation
    pub spinner_tick: u16,

    // Scroll debounce
    pub last_board_scroll: Option<Instant>,

    // Notion integration
    pub notify_rx: Option<std::sync::mpsc::Receiver<String>>,
    pub notion_pages_cache: Vec<NotionPage>,
    pub notion_busy: Option<String>,

    // Create task state
    pub create_input: String,
    pub create_tags: String,
    pub create_start_date: String,
    pub create_due_date: String,
    pub create_field: CreateField,
    pub create_priority_idx: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateField {
    Title,
    Tags,
    StartDate,
    DueDate,
}

pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
}

const PRIORITIES: [Priority; 4] = [
    Priority::Low,
    Priority::Medium,
    Priority::High,
    Priority::Critical,
];

/// Convert a persisted Session into UI SidebarMessages for display.
/// Parse a user-input date string into `DateTime<Utc>`.
/// Accepts `YYYY-MM-DD` or ISO8601 (`YYYY-MM-DDTHH:MM:SSZ`).
fn parse_date_input(input: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Try YYYY-MM-DD first
    if let Ok(naive) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let dt = naive
            .and_hms_opt(0, 0, 0)?;
        return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    // Try full ISO8601 / RFC3339
    chrono::DateTime::parse_from_rfc3339(trimmed)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok()
}

fn session_to_sidebar_messages(session: &icebox_runtime::Session) -> Vec<SidebarMessage> {
    let mut messages = Vec::new();
    for msg in &session.messages {
        let role = match msg.role {
            icebox_runtime::MessageRole::User => MessageRole::User,
            icebox_runtime::MessageRole::Assistant => MessageRole::Assistant,
            icebox_runtime::MessageRole::System | icebox_runtime::MessageRole::Tool => {
                MessageRole::System
            }
        };
        for block in &msg.blocks {
            let text = match block {
                icebox_runtime::ContentBlock::Text { text } => text.clone(),
                icebox_runtime::ContentBlock::ToolUse { name, input, .. } => {
                    let short: String = input.chars().take(200).collect();
                    format!("[tool: {name}] {short}")
                }
                icebox_runtime::ContentBlock::ToolResult {
                    tool_name,
                    output,
                    is_error,
                    ..
                } => {
                    let prefix = if *is_error { "ERROR" } else { "result" };
                    let short: String = output.chars().take(500).collect();
                    format!("[{tool_name} {prefix}] {short}")
                }
            };
            if !text.is_empty() {
                messages.push(SidebarMessage {
                    role: role.clone(),
                    content: text,
                });
            }
        }
    }
    messages
}

impl App {
    pub fn new(store: TaskStore, workspace: &std::path::Path) -> anyhow::Result<Self> {
        let tasks = store.tasks_by_column()?;
        let workspace_name = workspace
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| workspace.display().to_string());
        let git_branch = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(workspace)
            .output()
            .ok()
            .and_then(|o| {
                let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if branch.is_empty() { None } else { Some(branch) }
            });
        Ok(Self {
            mode: AppMode::Board,
            board: BoardState::new(tasks),
            sidebar: SidebarState::default(),
            sidebar_focused: false,
            store,
            should_quit: false,
            column_rects: vec![Rect::default(); 5],
            status_message: None,
            ai_rx: None,
            ai_sender: None,
            ai_busy: false,
            approval_tx: None,
            pending_tool_approval: None,
            workspace_name,
            workspace_path: workspace.to_path_buf(),
            git_branch,
            sidebar_rect: None,
            edit_title: String::new(),
            edit_body: String::new(),
            edit_field: EditField::Title,
            bottom_chat: SidebarState::default(),
            bottom_chat_open: false,
            bottom_chat_focused: false,
            bottom_chat_rect: None,
            bottom_chat_height: 12,
            drag_start_y: None,
            drag_target: DragTarget::None,
            sidebar_messages: HashMap::new(),
            active_task_id: None,
            ai_target_session: None,
            active_tab: Tab::Board,
            memory_store: icebox_task::memory::MemoryStore::open(workspace).ok(),
            memory_entries: Vec::new(),
            memory_selected: 0,
            memory_scroll: 0,
            current_model: icebox_runtime::DEFAULT_MODEL.to_string(),
            model_select_idx: 0,
            effort: icebox_runtime::Effort::High,
            create_input: String::new(),
            create_tags: String::new(),
            create_start_date: String::new(),
            create_due_date: String::new(),
            create_field: CreateField::Title,
            create_priority_idx: 1,
            spinner_tick: 0,
            last_board_scroll: None,
            notify_rx: None,
            notion_pages_cache: Vec::new(),
            notion_busy: None,
        })
    }

    pub fn set_ai_channel(
        &mut self,
        rx: mpsc::UnboundedReceiver<AiEvent>,
        sender: AiSender,
        approval_tx: mpsc::UnboundedSender<icebox_runtime::ToolApproval>,
    ) {
        self.ai_rx = Some(rx);
        self.ai_sender = Some(sender);
        self.approval_tx = Some(approval_tx);
    }

    pub fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>,
    ) -> anyhow::Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;

            // Poll AI events (non-blocking)
            self.drain_ai_events();
            self.drain_notify_events();

            if self.ai_busy || self.notion_busy.is_some() {
                self.spinner_tick = self.spinner_tick.wrapping_add(1);
            }

            if event::poll(Duration::from_millis(50))? {
                let ev = event::read()?;
                crate::input::handle_event(self, ev);
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn drain_ai_events(&mut self) {
        // Collect events first to avoid borrow issues
        let events: Vec<AiEvent> = match &mut self.ai_rx {
            Some(rx) => {
                let mut buf = Vec::new();
                while let Ok(ev) = rx.try_recv() {
                    buf.push(ev);
                }
                buf
            }
            None => return,
        };

        for event in events {
            self.process_ai_event(event);
        }
    }

    fn process_ai_event(&mut self, event: AiEvent) {
        match event {
            AiEvent::ToolApprovalRequest { name, input } => {
                self.pending_tool_approval = Some(PendingToolApproval {
                    tool_name: name,
                    tool_input: input,
                });
            }
            AiEvent::SessionContext { session_id } => {
                self.ai_target_session = Some(session_id);
            }
            AiEvent::Usage(usage) => {
                self.set_status(
                    format!(
                        "Tokens: {} in / {} out",
                        usage.input_tokens, usage.output_tokens
                    ),
                    false,
                );
            }
            AiEvent::TurnComplete => {
                self.ai_busy = false;
                self.ai_target_session = None;
                self.reload_tasks();
            }
            AiEvent::Error(msg) => {
                let target = self.ai_target_session.as_ref().and_then(|s| s.clone());
                let chat = self.resolve_chat_target(&target);
                chat.push(SidebarMessage {
                    role: MessageRole::System,
                    content: format!("Error: {msg}"),
                });
                self.ai_busy = false;
            }
            AiEvent::TextDelta(text) => {
                let target = self.ai_target_session.as_ref().and_then(|s| s.clone());
                let chat = self.resolve_chat_target(&target);
                if let Some(last) = chat.last_mut()
                    && matches!(last.role, MessageRole::Assistant)
                {
                    last.content.push_str(&text);
                    return;
                }
                chat.push(SidebarMessage {
                    role: MessageRole::Assistant,
                    content: text,
                });
            }
            AiEvent::ToolCallStart { name, input } => {
                let target = self.ai_target_session.as_ref().and_then(|s| s.clone());
                let chat = self.resolve_chat_target(&target);
                chat.push(SidebarMessage {
                    role: MessageRole::System,
                    content: format!("[tool: {name}] {input}"),
                });
            }
            AiEvent::ToolCallEnd {
                name,
                output,
                is_error,
            } => {
                let target = self.ai_target_session.as_ref().and_then(|s| s.clone());
                let chat = self.resolve_chat_target(&target);
                let prefix = if is_error { "ERROR" } else { "result" };
                chat.push(SidebarMessage {
                    role: MessageRole::System,
                    content: format!("[{name} {prefix}] {output}"),
                });
            }
        }
    }

    /// Resolve which message list to append AI events to, based on session target.
    fn resolve_chat_target(&mut self, session_id: &Option<String>) -> &mut Vec<SidebarMessage> {
        match session_id {
            // Global session → bottom chat
            None => &mut self.bottom_chat.messages,
            // Task session matching active task → sidebar
            Some(id) if self.active_task_id.as_ref() == Some(id) => &mut self.sidebar.messages,
            // Task session for a different task → buffer into sidebar_messages cache
            Some(id) => self.sidebar_messages.entry(id.clone()).or_default(),
        }
    }

    /// Switch the sidebar chat to a different task, saving/restoring messages.
    pub fn switch_sidebar_task(&mut self, new_task_id: Option<String>) {
        // Save current sidebar messages if we have an active task
        if let Some(old_id) = self.active_task_id.take() {
            let msgs = std::mem::take(&mut self.sidebar.messages);
            if !msgs.is_empty() {
                self.sidebar_messages.insert(old_id, msgs);
            }
        }

        // Restore messages for the new task
        if let Some(ref id) = new_task_id {
            let restored = self.sidebar_messages.remove(id).unwrap_or_else(|| {
                // Try loading from disk session file
                let path = icebox_runtime::session_path(&self.workspace_path, id);
                icebox_runtime::Session::load_from_path(&path)
                    .map(|session| session_to_sidebar_messages(&session))
                    .unwrap_or_default()
            });
            self.sidebar.messages = restored;
        } else {
            self.sidebar.messages.clear();
        }

        self.sidebar.detail_scroll = 0;
        self.sidebar.chat_scroll = 0;
        self.sidebar.chat_focused = false;
        self.active_task_id = new_task_id;
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let sidebar_open = matches!(
            self.mode,
            AppMode::TaskDetail | AppMode::EditTask | AppMode::SelectModel
        );
        let app_layout = layout::compute_layout(
            area,
            sidebar_open,
            self.bottom_chat_open,
            self.bottom_chat_height,
        );

        self.column_rects = app_layout.columns.clone();

        self.render_header(&app_layout, frame);

        match self.active_tab {
            Tab::Board => {
                self.board.render(&app_layout, frame.buffer_mut());

                // Sidebar (task detail / edit)
                self.sidebar_rect = app_layout.sidebar;
                if let Some(sidebar_area) = app_layout.sidebar {
                    if self.mode == AppMode::EditTask {
                        self.render_edit_task(sidebar_area, frame);
                    } else {
                        let task = self.board.selected_task().cloned();
                        sidebar::render_sidebar(
                            task.as_ref(),
                            &mut self.sidebar,
                            self.sidebar_focused,
                            self.ai_busy,
                            self.spinner_tick,
                            sidebar_area,
                            frame.buffer_mut(),
                        );

                        if self.sidebar_focused && !self.bottom_chat_focused {
                            // Input cursor: chat_area bottom - input border
                            if let Some(chat_rect) = self.sidebar.chat_rect {
                                let input_y =
                                    chat_rect.y + chat_rect.height.saturating_sub(3);
                                let display_width =
                                    self.sidebar.input.get(..self.sidebar.cursor_pos)
                                        .map_or(0, UnicodeWidthStr::width);
                                let input_x = chat_rect.x
                                    + 2
                                    + u16::try_from(display_width).unwrap_or(0);
                                frame.set_cursor_position((input_x, input_y));
                            }
                        }
                    }
                }
            }
            Tab::Memory => {
                self.render_memory_view(&app_layout, frame);
            }
        }

        // Bottom global chat
        self.bottom_chat_rect = app_layout.bottom_chat;
        if let Some(chat_area) = app_layout.bottom_chat {
            self.render_bottom_chat(chat_area, frame);

            if self.bottom_chat_focused {
                let input_y = chat_area.y + chat_area.height.saturating_sub(2);
                let display_width = self.bottom_chat.input.get(..self.bottom_chat.cursor_pos)
                    .map_or(0, UnicodeWidthStr::width);
                let input_x =
                    chat_area.x + 4 + u16::try_from(display_width).unwrap_or(0);
                frame.set_cursor_position((input_x, input_y));
            }
        }

        self.render_status_bar(&app_layout, frame);

        match self.mode {
            AppMode::CreateTask => self.render_create_modal(frame),
            AppMode::ConfirmDelete => self.render_delete_confirm(frame),
            AppMode::SelectModel => self.render_model_select(frame),
            AppMode::Board | AppMode::TaskDetail | AppMode::EditTask | AppMode::Memory => {}
        }

        // Slash command suggestion popup (render on top of everything)
        self.render_command_suggestions(frame);

        // Tool approval modal
        if let Some(pending) = &self.pending_tool_approval {
            let area = frame.area();
            let modal_w = 60.min(area.width.saturating_sub(4));
            let modal_h = 8;
            let x = (area.width.saturating_sub(modal_w)) / 2;
            let y = (area.height.saturating_sub(modal_h)) / 2;
            let modal_area = Rect::new(x, y, modal_w, modal_h);

            frame.render_widget(Clear, modal_area);

            let block = Block::default()
                .title(" Tool Approval ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ratatui::style::Color::Yellow));
            let inner = block.inner(modal_area);
            frame.render_widget(block, modal_area);

            let tool_display: String = pending.tool_name.chars().take(20).collect();
            let input_display: String = pending.tool_input.chars().take(
                inner.width.saturating_sub(2) as usize,
            ).collect();

            let lines = vec![
                Line::from(vec![
                    Span::styled("Tool: ", theme::dim_style()),
                    Span::styled(
                        tool_display,
                        Style::default()
                            .fg(ratatui::style::Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(input_display, theme::dim_style())),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" 1", Style::default().fg(ratatui::style::Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw(":Yes  "),
                    Span::styled("2", Style::default().fg(ratatui::style::Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(":Always  "),
                    Span::styled("3", Style::default().fg(ratatui::style::Color::Red).add_modifier(Modifier::BOLD)),
                    Span::raw(":No"),
                ]),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
        }
    }

    fn render_header(&self, layout: &AppLayout, frame: &mut Frame) {
        let mut header_spans = vec![
            Span::styled(
                concat!(" ICEBOX v", env!("CARGO_PKG_VERSION"), " "),
                Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", self.workspace_name),
                Style::default()
                    .fg(ratatui::style::Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " {} ",
                    std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                ),
                theme::dim_style(),
            ),
        ];
        if let Some(branch) = &self.git_branch {
            header_spans.push(Span::styled(
                "  ",
                Style::default().fg(ratatui::style::Color::DarkGray),
            ));
            header_spans.push(Span::styled(
                branch.clone(),
                Style::default()
                    .fg(ratatui::style::Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let header = Line::from(header_spans);
        frame.render_widget(Paragraph::new(header), layout.header);

        // Tab bar
        let tab_style = |tab: Tab| {
            if self.active_tab == tab {
                Style::default()
                    .fg(ratatui::style::Color::Black)
                    .bg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::dim_style()
            }
        };

        let tab_bar = Line::from(vec![
            Span::raw(" "),
            Span::styled(" 1:Board ", tab_style(Tab::Board)),
            Span::raw(" "),
            Span::styled(" 2:Memory ", tab_style(Tab::Memory)),
        ]);
        frame.render_widget(Paragraph::new(tab_bar), layout.tab_bar);
    }

    fn render_memory_view(&self, layout: &AppLayout, frame: &mut Frame) {
        // Use all column areas merged as the main content area
        let area = if let (Some(first), Some(last)) =
            (layout.columns.first(), layout.columns.last())
        {
            Rect::new(
                first.x,
                first.y,
                last.x + last.width - first.x,
                first.height,
            )
        } else {
            return;
        };

        let block = Block::default()
            .title(" Memory ")
            .borders(Borders::ALL)
            .border_style(theme::sidebar_border_style())
            .padding(Padding::uniform(1));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.memory_entries.is_empty() {
            let help = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No memories saved yet.",
                    theme::dim_style(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Use /remember <text> in chat to save a memory.",
                    theme::dim_style(),
                )),
                Line::from(Span::styled(
                    "Press 'n' to add manually, '1' to return to Board.",
                    theme::dim_style(),
                )),
            ]);
            frame.render_widget(help, inner);
            return;
        }

        let mut lines: Vec<Line<'_>> = Vec::new();
        for (i, entry) in self.memory_entries.iter().enumerate() {
            let is_selected = i == self.memory_selected;
            let date = entry.created_at.format("%m/%d %H:%M");
            let source_label = if entry.source == "global" {
                String::new()
            } else {
                let short: String = entry.source.chars().take(8).collect();
                format!(" ({short})")
            };

            let style = if is_selected {
                Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if is_selected { "> " } else { "  " };

            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(format!("[{date}]"), theme::dim_style()),
                Span::raw(" "),
                Span::styled(entry.content.clone(), style),
                Span::styled(source_label, theme::dim_style()),
            ]));
        }

        let scroll = self.memory_scroll;
        let paragraph = Paragraph::new(lines).scroll((scroll, 0));
        frame.render_widget(paragraph, inner);
    }

    fn render_status_bar(&self, layout: &AppLayout, frame: &mut Frame) {
        let mode_str = match self.mode {
            AppMode::Board if self.bottom_chat_focused => "BOARD [CHAT]",
            AppMode::Board => "BOARD",
            AppMode::TaskDetail if self.sidebar_focused => "DETAIL [INPUT]",
            AppMode::TaskDetail if self.bottom_chat_focused => "DETAIL [CHAT]",
            AppMode::TaskDetail => "DETAIL",
            AppMode::EditTask => match self.edit_field {
                EditField::Title => "EDIT [TITLE]",
                EditField::Body => "EDIT [BODY]",
            },
            AppMode::CreateTask => "CREATE",
            AppMode::ConfirmDelete => "DELETE?",
            AppMode::SelectModel => "MODEL",
            AppMode::Memory => "MEMORY",
        };

        // Show current model alias in status
        let model_alias =
            icebox_runtime::resolve_model(&self.current_model).map_or("custom", |m| m.alias);

        let task_count: usize = self.board.tasks.values().map(|v| v.len()).sum();
        let col = self.board.focused_col();

        let mut spans = vec![
            Span::styled(format!(" {mode_str} "), theme::status_bar_style()),
            Span::raw(" "),
            Span::styled(col.display_name(), theme::column_style(col, false)),
            Span::raw(format!("  Tasks: {task_count}")),
            Span::raw("  "),
            Span::styled(
                format!("[{model_alias}]"),
                Style::default().fg(ratatui::style::Color::Magenta),
            ),
            if self.ai_busy {
                Span::styled(
                    format!(" {} ", spinner_char(self.spinner_tick)),
                    Style::default().fg(ratatui::style::Color::Yellow),
                )
            } else {
                Span::raw(" ")
            },
        ];

        // Notion busy indicator — takes precedence over regular status message
        if let Some(label) = &self.notion_busy {
            let spinner = spinner_char(self.spinner_tick);
            spans.push(Span::styled(
                format!("{spinner} {label}..."),
                Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
            let status = Line::from(spans);
            frame.render_widget(Paragraph::new(status), layout.status_bar);
            return;
        }

        match &self.status_message {
            Some(msg) if msg.is_error => {
                spans.push(Span::styled(
                    &msg.text,
                    Style::default().fg(ratatui::style::Color::Red),
                ));
            }
            Some(msg) => {
                spans.push(Span::styled(
                    &msg.text,
                    Style::default().fg(ratatui::style::Color::Green),
                ));
            }
            None => {
                let chat_hint = if self.bottom_chat_open {
                    "/:close-chat"
                } else {
                    "/:chat"
                };
                spans.push(Span::styled(
                    format!("q:quit h/l:col j/k:task Enter:detail n:new d:del >/<:move r:reload {chat_hint}"),
                    theme::dim_style(),
                ));
            }
        }

        let status = Line::from(spans);
        frame.render_widget(Paragraph::new(status), layout.status_bar);
    }

    fn render_edit_task(&self, area: Rect, frame: &mut Frame) {
        use ratatui::widgets::Wrap;

        let block = Block::default()
            .title(" Edit Task (Tab: switch field, Ctrl+S: save, Esc: cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ratatui::style::Color::Yellow))
            .padding(Padding::uniform(1));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let layout = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Length(3),
                ratatui::layout::Constraint::Min(1),
            ])
            .split(inner);

        let Some(title_area) = layout.first().copied() else {
            return;
        };
        let Some(body_area) = layout.get(1).copied() else {
            return;
        };

        // Title field
        let title_border = if self.edit_field == EditField::Title {
            Style::default().fg(ratatui::style::Color::Cyan)
        } else {
            theme::dim_style()
        };
        let title_block = Block::default()
            .title(" Title ")
            .borders(Borders::ALL)
            .border_style(title_border);
        let title_inner = title_block.inner(title_area);
        frame.render_widget(title_block, title_area);
        let title_line = Line::from(Span::styled(&self.edit_title, theme::input_style()));
        frame.render_widget(Paragraph::new(title_line), title_inner);

        // Body field
        let body_border = if self.edit_field == EditField::Body {
            Style::default().fg(ratatui::style::Color::Cyan)
        } else {
            theme::dim_style()
        };
        let body_block = Block::default()
            .title(" Body (markdown) ")
            .borders(Borders::ALL)
            .border_style(body_border);
        let body_inner = body_block.inner(body_area);
        frame.render_widget(body_block, body_area);

        let body_lines: Vec<Line<'_>> = self
            .edit_body
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect();
        let body_para = Paragraph::new(body_lines).wrap(Wrap { trim: false });
        frame.render_widget(body_para, body_inner);

        // Cursor position
        match self.edit_field {
            EditField::Title => {
                let cx =
                    title_inner.x + u16::try_from(UnicodeWidthStr::width(self.edit_title.as_str())).unwrap_or(0);
                frame.set_cursor_position((cx, title_inner.y));
            }
            EditField::Body => {
                let line_count = self.edit_body.lines().count();
                let last_line_len = self
                    .edit_body
                    .lines()
                    .last()
                    .map_or(0, UnicodeWidthStr::width);
                let cy = body_inner.y + u16::try_from(line_count.saturating_sub(1)).unwrap_or(0);
                let cx = body_inner.x + u16::try_from(last_line_len).unwrap_or(0);
                frame.set_cursor_position((
                    cx.min(body_inner.x + body_inner.width.saturating_sub(1)),
                    cy.min(body_inner.y + body_inner.height.saturating_sub(1)),
                ));
            }
        }
    }

    pub fn start_edit_task(&mut self) {
        if let Some(task) = self.board.selected_task() {
            self.edit_title = task.title.clone();
            self.edit_body = task.body.clone();
            self.edit_field = EditField::Title;
            self.mode = AppMode::EditTask;
        }
    }

    pub fn save_edit_task(&mut self) {
        let Some(task) = self.board.selected_task().cloned() else {
            return;
        };
        let mut task = task;
        task.title = self.edit_title.trim().to_string();
        task.body = self.edit_body.clone();
        task.updated_at = Utc::now();
        match self.store.save(&task) {
            Ok(()) => {
                self.reload_tasks();
                self.set_status(format!("Saved: {}", task.title), false);
                self.mode = AppMode::TaskDetail;
            }
            Err(e) => {
                self.set_status(format!("Save failed: {e}"), true);
            }
        }
    }

    fn render_model_select(&self, frame: &mut Frame) {
        let area = frame.area();
        let modal_w = 75.min(area.width.saturating_sub(4));
        let model_count = icebox_runtime::MODELS.len() as u16;
        let modal_h = (model_count * 2 + 10).min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(modal_w)) / 2;
        let y = (area.height.saturating_sub(modal_h)) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ratatui::style::Color::DarkGray))
            .padding(Padding::new(2, 2, 1, 1));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let mut lines: Vec<Line<'_>> = Vec::new();

        // Title
        lines.push(Line::from(Span::styled(
            "Select model",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            "Switch between Claude models. Applies to this session.",
            theme::dim_style(),
        )));
        lines.push(Line::from(""));

        // Model list
        for (i, model) in icebox_runtime::MODELS.iter().enumerate() {
            let is_current = self.current_model == model.id;
            let is_selected = i == self.model_select_idx;

            let marker = if is_selected { "❯" } else { " " };
            let check = if is_current { " ✔" } else { "" };
            let default_tag = if model.is_default {
                " (recommended)"
            } else {
                ""
            };

            let num_style = theme::dim_style();
            let name_style = if is_selected {
                Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("  {marker} "), name_style),
                Span::styled(format!("{}. ", i + 1), num_style),
                Span::styled(model.display_name, name_style),
                Span::styled(default_tag, theme::dim_style()),
                Span::styled(check, Style::default().fg(ratatui::style::Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("       "),
                Span::styled(
                    format!(
                        "{} · ${:.0}→${:.0}/M · {}K tokens",
                        model.description,
                        model.input_per_million,
                        model.output_per_million,
                        model.max_tokens / 1000,
                    ),
                    theme::dim_style(),
                ),
            ]));
        }

        // Effort selector
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                self.effort.indicator(),
                Style::default().fg(ratatui::style::Color::Cyan),
            ),
            Span::styled(
                format!("  {} effort", self.effort.label()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ← → to adjust", theme::dim_style()),
        ]));

        // Footer
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Enter to confirm · Esc to exit",
            theme::dim_style(),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    pub fn apply_model_selection(&mut self) {
        if let Some(model) = icebox_runtime::MODELS.get(self.model_select_idx) {
            self.current_model = model.id.to_string();
            let _ = icebox_runtime::IceboxConfig::save_model(model.id);
            if let Some(sender) = &self.ai_sender {
                sender(RuntimeCommand::SwitchModel(model.id.to_string()));
            }
            self.set_status(
                format!(
                    "Model: {} ({}, {}K tokens)",
                    model.alias,
                    model.id,
                    model.max_tokens / 1000
                ),
                false,
            );
        }
        self.mode = AppMode::Board;
    }

    fn render_bottom_chat(&mut self, area: Rect, frame: &mut Frame) {
        use ratatui::widgets::Wrap;

        let border_style = if self.bottom_chat_focused {
            Style::default().fg(ratatui::style::Color::Cyan)
        } else {
            theme::dim_style()
        };

        let model_alias =
            icebox_runtime::resolve_model(&self.current_model).map_or("custom", |m| m.alias);
        let title = if self.ai_busy {
            let spinner = spinner_char(self.spinner_tick);
            format!(" AI Chat [{model_alias}] {spinner} thinking... ")
        } else {
            format!(" AI Chat [{model_alias}] ")
        };

        let mut block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style)
            .padding(Padding::horizontal(1));

        if self.bottom_chat_focused {
            block = block.title_bottom(
                Line::from(Span::styled(
                    " Ctrl+↑↓: resize ",
                    theme::dim_style(),
                ))
                .alignment(ratatui::layout::Alignment::Right),
            );
        }

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 2 {
            return;
        }

        // Split inner: messages area + input line
        let layout = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Min(1),
                ratatui::layout::Constraint::Length(1),
            ])
            .split(inner);

        let Some(msg_area) = layout.first().copied() else {
            return;
        };
        let Some(input_area) = layout.get(1).copied() else {
            return;
        };

        // Render messages with color-coded indicators
        let mut lines: Vec<Line<'_>> = Vec::new();
        for msg in &self.bottom_chat.messages {
            match msg.role {
                MessageRole::User => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "⏺ ",
                            Style::default().fg(ratatui::style::Color::Cyan),
                        ),
                        Span::styled(
                            &msg.content,
                            Style::default()
                                .fg(ratatui::style::Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
                MessageRole::Assistant => {
                    sidebar::render_markdown_lines(&msg.content, &mut lines);
                }
                MessageRole::System => {
                    lines.push(sidebar::render_system_message_line(&msg.content));
                }
            }
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Type a message to manage tasks with AI. Try: \"list all tasks\" or \"/help\"",
                theme::dim_style(),
            )));
        }

        self.bottom_chat.rendered_chat_lines = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        if self.ai_busy {
            let spinner = spinner_char(self.spinner_tick);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {spinner} "),
                    Style::default().fg(ratatui::style::Color::Yellow),
                ),
                Span::styled(
                    "Thinking...",
                    Style::default()
                        .fg(ratatui::style::Color::Yellow)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.bottom_chat.chat_scroll, 0));
        frame.render_widget(paragraph, msg_area);

        // Render input line
        let prompt = if self.bottom_chat_focused { "> " } else { "  " };
        let input_line = Line::from(vec![
            Span::styled(prompt, Style::default().fg(ratatui::style::Color::Cyan)),
            Span::styled(&self.bottom_chat.input, theme::input_style()),
        ]);
        frame.render_widget(Paragraph::new(input_line), input_area);
    }

    fn render_command_suggestions(&self, frame: &mut Frame) {
        // Determine which input is active and its anchor rect
        let (active_input, anchor_rect) = if self.bottom_chat_focused {
            if let Some(r) = self.bottom_chat_rect {
                (&self.bottom_chat.input, r)
            } else {
                return;
            }
        } else if self.sidebar_focused {
            if let Some(r) = self.sidebar.chat_rect {
                (&self.sidebar.input, r)
            } else {
                return;
            }
        } else {
            return;
        };

        // Only show suggestions when input starts with "/"
        if !active_input.starts_with('/') {
            return;
        }

        let matches = icebox_commands::filter_commands(active_input);
        if matches.is_empty() {
            return;
        }

        // Build lines first to know true content height
        let mut lines: Vec<Line<'_>> = Vec::new();
        let mut current_cat: Option<icebox_commands::CommandCategory> = None;

        for spec in &matches {
            if current_cat != Some(spec.category) {
                if current_cat.is_some() {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!("  {}", spec.category.label()),
                    Style::default()
                        .fg(ratatui::style::Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )));
                current_cat = Some(spec.category);
            }

            let name = match spec.argument_hint {
                Some(hint) => format!("/{} {hint}", spec.name),
                None => format!("/{}", spec.name),
            };

            let aliases = if spec.aliases.is_empty() {
                String::new()
            } else {
                let a: Vec<&str> = spec.aliases.to_vec();
                format!(" ({})", a.join(","))
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {name:<24}"),
                    Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(spec.summary, theme::dim_style()),
                Span::styled(
                    aliases,
                    Style::default().fg(ratatui::style::Color::DarkGray),
                ),
            ]));
        }

        let area = frame.area();
        // +2 for border top/bottom
        let content_h = lines.len() as u16 + 2;
        let max_h = anchor_rect.y.saturating_sub(area.y).min(area.height.saturating_sub(4));
        let popup_h = content_h.min(max_h).max(4);
        let popup_w = 60.min(area.width.saturating_sub(2));

        // Position: just above the anchor rect, aligned to its x
        let popup_y = anchor_rect.y.saturating_sub(popup_h);
        let popup_x = anchor_rect.x.min(area.width.saturating_sub(popup_w));

        let popup_area = Rect::new(popup_x, popup_y, popup_w, popup_h);
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Commands ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ratatui::style::Color::Cyan))
            .padding(Padding::horizontal(1));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    pub fn handle_bottom_chat_input(&mut self, input: String) {
        // Handle slash commands
        if let Some(cmd) = icebox_commands::SlashCommand::parse(&input) {
            self.handle_slash_command(cmd);
            return;
        }

        self.bottom_chat.messages.push(SidebarMessage {
            role: MessageRole::User,
            content: input.clone(),
        });

        if self.ai_busy {
            self.bottom_chat.messages.push(SidebarMessage {
                role: MessageRole::System,
                content: "AI is busy, please wait...".into(),
            });
            return;
        }

        match &self.ai_sender {
            Some(sender) => {
                self.ai_busy = true;
                sender(RuntimeCommand::SendMessage {
                    session_id: None,
                    input,
                });
            }
            None => {
                self.bottom_chat.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: "AI not connected. Set ANTHROPIC_API_KEY or run `icebox login`."
                        .into(),
                });
            }
        }
    }

    fn render_create_modal(&self, frame: &mut Frame) {
        let area = frame.area();
        let modal_w = 55.min(area.width.saturating_sub(4));
        let modal_h = 14;
        let x = (area.width.saturating_sub(modal_w)) / 2;
        let y = (area.height.saturating_sub(modal_h)) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        frame.render_widget(Clear, modal_area);

        let priority = self.current_create_priority();
        let block = Block::default()
            .title(" New Task (Enter: next/create, Esc: cancel) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ratatui::style::Color::Cyan))
            .padding(Padding::horizontal(1));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let cursor = |field: CreateField| -> &str {
            if self.create_field == field { "_" } else { "" }
        };
        let label_style = |field: CreateField| -> Style {
            if self.create_field == field {
                Style::default().fg(ratatui::style::Color::Cyan)
            } else {
                theme::dim_style()
            }
        };
        let blink = Style::default().add_modifier(Modifier::SLOW_BLINK);

        let tags_hint = if self.create_tags.is_empty() && self.create_field == CreateField::Tags {
            "comma separated: bug, ui, backend"
        } else {
            ""
        };
        let start_hint = if self.create_start_date.is_empty() && self.create_field == CreateField::StartDate {
            "YYYY-MM-DD (optional)"
        } else {
            ""
        };
        let due_hint = if self.create_due_date.is_empty() && self.create_field == CreateField::DueDate {
            "YYYY-MM-DD (optional)"
        } else {
            ""
        };

        let lines = vec![
            Line::from(vec![
                Span::styled("Title:    ", label_style(CreateField::Title)),
                Span::raw(&self.create_input),
                Span::styled(cursor(CreateField::Title), blink),
            ]),
            Line::from(vec![
                Span::styled("Tags:     ", label_style(CreateField::Tags)),
                Span::raw(&self.create_tags),
                Span::styled(cursor(CreateField::Tags), blink),
                Span::styled(tags_hint, theme::dim_style()),
            ]),
            Line::from(vec![
                Span::styled("Start:    ", label_style(CreateField::StartDate)),
                Span::raw(&self.create_start_date),
                Span::styled(cursor(CreateField::StartDate), blink),
                Span::styled(start_hint, theme::dim_style()),
            ]),
            Line::from(vec![
                Span::styled("Due:      ", label_style(CreateField::DueDate)),
                Span::raw(&self.create_due_date),
                Span::styled(cursor(CreateField::DueDate), blink),
                Span::styled(due_hint, theme::dim_style()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Priority (Tab): ", theme::dim_style()),
                Span::styled(
                    format!("{} {}", priority.symbol(), priority.label()),
                    theme::priority_style(priority),
                ),
            ]),
            Line::from(vec![
                Span::styled("Column: ", theme::dim_style()),
                Span::styled(
                    self.board.focused_col().display_name(),
                    theme::column_style(self.board.focused_col(), false),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "↑↓: switch field  Tab: priority  Enter: next/create",
                theme::dim_style(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_delete_confirm(&self, frame: &mut Frame) {
        let area = frame.area();
        let modal_w = 40.min(area.width.saturating_sub(4));
        let modal_h = 5;
        let x = (area.width.saturating_sub(modal_w)) / 2;
        let y = (area.height.saturating_sub(modal_h)) / 2;
        let modal_area = Rect::new(x, y, modal_w, modal_h);

        frame.render_widget(Clear, modal_area);

        let task_title = match self.board.selected_task() {
            Some(t) => t.title.as_str(),
            None => "(none)",
        };

        let block = Block::default()
            .title(" Confirm Delete ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ratatui::style::Color::Red))
            .padding(Padding::horizontal(1));

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let lines = vec![
            Line::from(format!("Delete \"{task_title}\"?")),
            Line::from(""),
            Line::from(Span::styled(
                "y: confirm / any key: cancel",
                theme::dim_style(),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn current_create_priority(&self) -> Priority {
        PRIORITIES
            .get(self.create_priority_idx)
            .copied()
            .unwrap_or(Priority::Medium)
    }

    pub fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status_message = Some(StatusMessage {
            text: text.into(),
            is_error,
        });
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn reload_tasks(&mut self) {
        match self.store.tasks_by_column() {
            Ok(tasks) => {
                self.board.reload(tasks);
                self.clear_status();
            }
            Err(e) => {
                self.set_status(format!("Reload failed: {e}"), true);
            }
        }
    }

    pub fn respond_tool_approval(&mut self, approval: icebox_runtime::ToolApproval) {
        if let Some(tx) = &self.approval_tx {
            let _ = tx.send(approval);
        }
        self.pending_tool_approval = None;
    }

    pub fn reload_memory(&mut self) {
        if let Some(store) = &self.memory_store {
            match store.list() {
                Ok(entries) => self.memory_entries = entries,
                Err(e) => self.set_status(format!("Memory load failed: {e}"), true),
            }
        }
    }

    pub fn create_task_from_input(&mut self) {
        let title = self.create_input.trim().to_string();
        if title.is_empty() {
            self.set_status("Task title cannot be empty", true);
            return;
        }
        let priority = self.current_create_priority();
        let column = self.board.focused_col();
        let mut task = Task::new(title.clone(), column, priority);

        // Parse tags from comma-separated input
        let tags: Vec<String> = self
            .create_tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        task.tags = tags;

        if let Some(dt) = parse_date_input(&self.create_start_date) {
            task.start_date = Some(dt);
        }
        if let Some(dt) = parse_date_input(&self.create_due_date) {
            task.due_date = Some(dt);
        }

        match self.store.save(&task) {
            Ok(()) => {
                self.reload_tasks();
                self.set_status(format!("Created: {title}"), false);
            }
            Err(e) => {
                self.set_status(format!("Create failed: {e}"), true);
            }
        }
    }

    pub fn delete_selected_task(&mut self) {
        let Some(task) = self.board.selected_task().cloned() else {
            self.set_status("No task selected", true);
            return;
        };
        let title = task.title.clone();
        match self.store.delete(&task.id) {
            Ok(()) => {
                self.reload_tasks();
                self.set_status(format!("Deleted: {title}"), false);
            }
            Err(e) => {
                self.set_status(format!("Delete failed: {e}"), true);
            }
        }
    }

    pub fn move_task_right(&mut self) {
        self.move_selected_task(|col| col.next());
    }

    pub fn move_task_left(&mut self) {
        self.move_selected_task(|col| col.prev());
    }

    fn move_selected_task(&mut self, direction: impl FnOnce(Column) -> Option<Column>) {
        let Some(task) = self.board.selected_task().cloned() else {
            return;
        };
        let Some(new_col) = direction(task.column) else {
            return;
        };
        let mut task = task;
        task.column = new_col;
        task.updated_at = Utc::now();
        match self.store.save(&task) {
            Ok(()) => {
                self.reload_tasks();
                self.set_status(format!("Moved to {}", new_col.display_name()), false);
            }
            Err(e) => {
                self.set_status(format!("Move failed: {e}"), true);
            }
        }
    }

    pub fn handle_sidebar_input(&mut self, input: String) {
        // Handle slash commands
        if let Some(cmd) = icebox_commands::SlashCommand::parse(&input) {
            self.handle_slash_command(cmd);
            return;
        }

        self.sidebar.messages.push(SidebarMessage {
            role: MessageRole::User,
            content: input.clone(),
        });

        if self.ai_busy {
            self.sidebar.messages.push(SidebarMessage {
                role: MessageRole::System,
                content: "AI is busy, please wait...".into(),
            });
            return;
        }

        match &self.ai_sender {
            Some(sender) => {
                self.ai_busy = true;
                let task = self.board.selected_task().cloned();
                let task_id = task.as_ref().map(|t| t.id.clone());

                // Prepend task context on first message of a session
                let enriched_input = if self.sidebar.messages.len() <= 1 {
                    if let Some(t) = &task {
                        format!(
                            "[Task Context]\nTitle: {}\nStatus: {}\nPriority: {}\n{}{}{}\n\n{input}",
                            t.title,
                            t.column.display_name(),
                            t.priority.label(),
                            if t.tags.is_empty() {
                                String::new()
                            } else {
                                format!("Tags: {}\n", t.tags.join(", "))
                            },
                            if t.body.is_empty() {
                                String::new()
                            } else {
                                format!("Body:\n{}\n", t.body)
                            },
                            if t.depends_on.is_empty() {
                                String::new()
                            } else {
                                format!("Depends on: {}\n", t.depends_on.join(", "))
                            },
                        )
                    } else {
                        input
                    }
                } else {
                    input
                };

                sender(RuntimeCommand::SendMessage {
                    session_id: task_id,
                    input: enriched_input,
                });
            }
            None => {
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: "AI not connected. Set ANTHROPIC_API_KEY to enable.".into(),
                });
            }
        }
    }

    fn handle_notion_command(&mut self, action: Option<String>) {
        let action_str = action.as_deref().unwrap_or("").trim();

        // Parse subcommand
        let (subcmd, arg) = match action_str.split_once(' ') {
            Some((cmd, rest)) => (cmd.trim(), Some(rest.trim())),
            None => (action_str, None),
        };

        match subcmd {
            "push" => self.notion_push(arg),
            "pull" => self.notion_pull(),
            "status" => self.notion_status(),
            "reset" => self.notion_reset(),
            "" => {
                let config = icebox_runtime::IceboxConfig::load();
                let has_env_key = std::env::var("NOTION_API_KEY")
                    .map(|k| !k.is_empty())
                    .unwrap_or(false);
                let has_config_key = config
                    .notion
                    .as_ref()
                    .and_then(|n| n.api_key.as_ref())
                    .is_some();
                let has_db = config
                    .notion
                    .as_ref()
                    .and_then(|n| n.database_id.as_ref())
                    .is_some();

                // Verify API key if present
                let key_source = if has_env_key {
                    Some("env (NOTION_API_KEY)")
                } else if has_config_key {
                    Some("config.json")
                } else {
                    None
                };

                let key_status = match key_source {
                    Some(source) => {
                        let config_key = config
                            .notion
                            .as_ref()
                            .and_then(|n| n.api_key.as_deref())
                            .map(String::from);
                        match icebox_tools::notion::NotionClient::from_env(
                            config_key.as_deref(),
                        ) {
                            Ok(client) => match client.verify_key() {
                                Ok(name) => {
                                    format!("  API Key: {source} — valid (bot: {name})")
                                }
                                Err(e) => format!("  API Key: {source} — invalid ({e})"),
                            },
                            Err(e) => format!("  API Key: {source} — error ({e})"),
                        }
                    }
                    None => "  API Key: not configured".to_string(),
                };

                let db_status = if has_db {
                    let db_id = config
                        .notion
                        .as_ref()
                        .and_then(|n| n.database_id.as_deref())
                        .unwrap_or("-");
                    format!("  Database: {db_id}")
                } else {
                    "  Database: not configured".to_string()
                };

                let setup_guide = "\n[Setup]\n\
                     1. Create an integration at https://www.notion.so/my-integrations\n\
                     2. Add NOTION_API_KEY to your shell profile:\n\
                        zsh:  echo 'export NOTION_API_KEY=ntn_...' >> ~/.zshrc\n\
                        bash: echo 'export NOTION_API_KEY=ntn_...' >> ~/.bashrc\n\
                        fish: set -Ux NOTION_API_KEY ntn_...\n\
                     3. Invite the integration to your Notion page\n\
                     4. Run /notion push <page-name> to start syncing";

                let content = format!(
                    "Notion Integration\n\
                     {key_status}\n\
                     {db_status}\n\
                     {setup_guide}\n\n\
                     Commands:\n\
                     /notion push [page]  Sync local tasks → Notion\n\
                     /notion pull         Sync Notion → local tasks\n\
                     /notion status       Show connection status\n\
                     /notion reset        Clear configuration"
                );

                let chat = self.active_chat_messages();
                chat.push(SidebarMessage {
                    role: MessageRole::System,
                    content,
                });
            }
            other => {
                // Treat as "/notion push <other>" shorthand
                self.notion_push(Some(other));
            }
        }
    }

    fn notion_push(&mut self, page_selector: Option<&str>) {
        let config = icebox_runtime::IceboxConfig::load();
        let config_api_key = config
            .notion
            .as_ref()
            .and_then(|n| n.api_key.as_deref())
            .map(String::from);

        // Check API key availability
        let has_env_key = std::env::var("NOTION_API_KEY")
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        if !has_env_key && config_api_key.is_none() {
            let chat = self.active_chat_messages();
            chat.push(SidebarMessage {
                role: MessageRole::System,
                content: "NOTION_API_KEY is not set.\n\
                          Create one at https://www.notion.so/my-integrations\n\
                          Type /notion for setup instructions."
                    .into(),
            });
            return;
        }

        // If we have a database_id and no page selector, sync directly
        if page_selector.is_none()
            && let Some(ref notion) = config.notion
            && let Some(ref db_id) = notion.database_id
        {
            let db_id = db_id.clone();
            let tasks = self.store.list().unwrap_or_default();
            self.set_status("Syncing to Notion...", false);
            self.notion_busy = Some("Syncing to Notion".to_string());

            let (tx, rx) = std::sync::mpsc::channel();
            self.notify_rx = Some(rx);

            std::thread::spawn(move || {
                let client = match icebox_tools::notion::NotionClient::from_env(
                    config_api_key.as_deref(),
                ) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(format!("Notion connection failed: {e}"));
                        return;
                    }
                };
                match client.sync_tasks(&db_id, &tasks) {
                    Ok(result) => {
                        let _ = tx.send(result.to_string());
                    }
                    Err(e) => {
                        let _ = tx.send(format!("Notion sync failed: {e}"));
                    }
                }
            });
            return;
        }

        // No database configured or page selector provided → search/setup
        match page_selector {
            Some(selector) => {
                // Check if it's a number (referencing cached search results)
                if let Ok(idx) = selector.parse::<usize>() {
                    let cache_len = self.notion_pages_cache.len();
                    if idx == 0 || idx > cache_len {
                        let chat = self.active_chat_messages();
                        chat.push(SidebarMessage {
                            role: MessageRole::System,
                            content: format!(
                                "Invalid number. Choose from 1 to {cache_len}.",
                            ),
                        });
                        return;
                    }
                    let page = self.notion_pages_cache.get(idx - 1).cloned();
                    if let Some(page) = page {
                        self.notion_setup_and_sync(page, config_api_key);
                    }
                    return;
                }

                // Search by name
                self.set_status("Searching Notion pages...", false);
                self.notion_busy = Some("Searching Notion pages".to_string());
                let query = selector.to_string();
                let tasks = self.store.list().unwrap_or_default();

                let (tx, rx) = std::sync::mpsc::channel();
                self.notify_rx = Some(rx);

                std::thread::spawn(move || {
                    let client = match icebox_tools::notion::NotionClient::from_env(
                        config_api_key.as_deref(),
                    ) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tx.send(format!("Notion connection failed: {e}"));
                            return;
                        }
                    };

                    // Search for pages
                    let pages = match client.search_pages(&query) {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = tx.send(format!("Notion search failed: {e}"));
                            return;
                        }
                    };

                    if pages.is_empty() {
                        let _ = tx.send(format!(
                            "No Notion pages matching '{query}'.\n\
                             \n\
                             Make sure the integration is invited to the page:\n\
                             1. Open the Notion page you want to connect\n\
                             2. Click '...' in the top-right corner\n\
                             3. Go to 'Connections' and add your integration\n\
                             4. Run /notion push <page-name> again"
                        ));
                        return;
                    }

                    // If exactly one match, use it directly
                    if pages.len() == 1 {
                        let page = &pages[0];
                        let _ = tx.send(format!(
                            "Creating database under Notion page '{}'...",
                            page.title
                        ));
                        match client.create_database(&page.id) {
                            Ok(db_id) => {
                                if let Err(e) =
                                    icebox_runtime::IceboxConfig::save_notion(&db_id, &page.id)
                                {
                                    let _ = tx.send(format!("Failed to save config: {e}"));
                                    return;
                                }
                                match client.sync_tasks(&db_id, &tasks) {
                                    Ok(result) => {
                                        let _ = tx.send(result.to_string());
                                    }
                                    Err(e) => {
                                        let _ = tx.send(format!("Notion sync failed: {e}"));
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(format!("Notion DB creation failed: {e}"));
                            }
                        }
                        return;
                    }

                    // Multiple matches → show list
                    let mut msg = String::from("Multiple pages matched:\n");
                    for (i, page) in pages.iter().enumerate() {
                        msg.push_str(&format!("  {}. {}\n", i + 1, page.title));
                    }
                    msg.push_str("\nSelect with /notion push <number>");
                    // Send page list as JSON for caching
                    let _ = tx.send(format!("__PAGES__:{}", serde_json::to_string(&pages.iter().map(|p| (&p.id, &p.title)).collect::<Vec<_>>()).unwrap_or_default()));
                    let _ = tx.send(msg);
                });
            }
            None => {
                // No config, no selector → search all and show list
                self.set_status("Searching Notion pages...", false);
                self.notion_busy = Some("Searching Notion pages".to_string());

                let (tx, rx) = std::sync::mpsc::channel();
                self.notify_rx = Some(rx);

                std::thread::spawn(move || {
                    let client = match icebox_tools::notion::NotionClient::from_env(
                        config_api_key.as_deref(),
                    ) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tx.send(format!("Notion connection failed: {e}"));
                            return;
                        }
                    };

                    let pages = match client.search_pages("") {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = tx.send(format!("Notion search failed: {e}"));
                            return;
                        }
                    };

                    if pages.is_empty() {
                        let _ = tx.send(
                            "No Notion pages accessible.\n\
                             Make sure the integration is invited to at least one page."
                                .to_string(),
                        );
                        return;
                    }

                    let mut msg = String::from("Notion pages:\n");
                    for (i, page) in pages.iter().enumerate() {
                        msg.push_str(&format!("  {}. {}\n", i + 1, page.title));
                    }
                    msg.push_str("\nSelect a target page with /notion push <number>");
                    let _ = tx.send(format!("__PAGES__:{}", serde_json::to_string(&pages.iter().map(|p| (&p.id, &p.title)).collect::<Vec<_>>()).unwrap_or_default()));
                    let _ = tx.send(msg);
                });
            }
        }
    }

    fn notion_pull(&mut self) {
        let config = icebox_runtime::IceboxConfig::load();
        let config_api_key = config
            .notion
            .as_ref()
            .and_then(|n| n.api_key.as_deref())
            .map(String::from);

        let db_id = match config
            .notion
            .as_ref()
            .and_then(|n| n.database_id.as_deref())
        {
            Some(id) => id.to_owned(),
            None => {
                let chat = self.active_chat_messages();
                chat.push(SidebarMessage {
                    role: MessageRole::System,
                    content: "No database configured.\n\
                              Run /notion push <page-name> first."
                        .into(),
                });
                return;
            }
        };

        let local_tasks = self.store.list().unwrap_or_default();
        let workspace = self.workspace_path.clone();
        self.set_status("Pulling from Notion...", false);
        self.notion_busy = Some("Pulling from Notion".to_string());

        let (tx, rx) = std::sync::mpsc::channel();
        self.notify_rx = Some(rx);

        std::thread::spawn(move || {
            let client = match icebox_tools::notion::NotionClient::from_env(
                config_api_key.as_deref(),
            ) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(format!("Notion connection failed: {e}"));
                    return;
                }
            };

            let remote_tasks = match client.pull_tasks(&db_id) {
                Ok(t) => t,
                Err(e) => {
                    let _ = tx.send(format!("Notion pull failed: {e}"));
                    return;
                }
            };

            let store = match icebox_task::store::TaskStore::open(&workspace) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(format!("Failed to open task store: {e}"));
                    return;
                }
            };

            let local_map: std::collections::HashMap<String, icebox_task::model::Task> =
                local_tasks.into_iter().map(|t| (t.id.clone(), t)).collect();

            let mut created = 0;
            let mut updated = 0;
            let mut unchanged = 0;
            let mut errors: Vec<String> = Vec::new();
            let mut remote_ids = std::collections::HashSet::new();

            for (mut remote, last_edited) in remote_tasks {
                remote_ids.insert(remote.id.clone());
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&last_edited) {
                    remote.updated_at = dt.with_timezone(&chrono::Utc);
                }
                match local_map.get(&remote.id) {
                    None => match store.save(&remote) {
                        Ok(()) => created += 1,
                        Err(e) => errors.push(format!("{}: {e}", remote.id)),
                    },
                    Some(local) => {
                        if remote.updated_at > local.updated_at {
                            match store.save(&remote) {
                                Ok(()) => updated += 1,
                                Err(e) => errors.push(format!("{}: {e}", remote.id)),
                            }
                        } else {
                            unchanged += 1;
                        }
                    }
                }
            }

            // Delete local tasks missing from Notion (Notion is source of truth)
            let mut deleted = 0;
            let mut deleted_titles: Vec<String> = Vec::new();
            for (id, local) in &local_map {
                if !remote_ids.contains(id) {
                    match store.delete(id) {
                        Ok(()) => {
                            deleted += 1;
                            deleted_titles.push(local.title.clone());
                        }
                        Err(e) => errors.push(format!("delete {id}: {e}")),
                    }
                }
            }

            let mut msg = format!(
                "Pull complete: {created} created, {updated} updated, {unchanged} unchanged, {deleted} deleted"
            );
            if !deleted_titles.is_empty() {
                msg.push_str("\n\nDeleted locally (removed from Notion):");
                for title in &deleted_titles {
                    msg.push_str(&format!("\n  - {title}"));
                }
            }
            if !errors.is_empty() {
                msg.push_str("\n\nErrors:");
                for e in &errors {
                    msg.push_str(&format!("\n  {e}"));
                }
            }

            // Signal reload-needed so TUI picks up new tasks
            let _ = tx.send("__RELOAD__".to_string());
            let _ = tx.send(msg);
        });
    }

    fn notion_setup_and_sync(
        &mut self,
        page: NotionPage,
        config_api_key: Option<String>,
    ) {
        let tasks = self.store.list().unwrap_or_default();
        self.set_status("Creating Notion DB and syncing...", false);
        self.notion_busy = Some("Creating Notion DB and syncing".to_string());

        let (tx, rx) = std::sync::mpsc::channel();
        self.notify_rx = Some(rx);

        std::thread::spawn(move || {
            let client = match icebox_tools::notion::NotionClient::from_env(
                config_api_key.as_deref(),
            ) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(format!("Notion connection failed: {e}"));
                    return;
                }
            };

            let _ = tx.send(format!(
                "Creating Icebox Kanban database under '{}'...",
                page.title
            ));

            match client.create_database(&page.id) {
                Ok(db_id) => {
                    if let Err(e) =
                        icebox_runtime::IceboxConfig::save_notion(&db_id, &page.id)
                    {
                        let _ = tx.send(format!("Failed to save config: {e}"));
                        return;
                    }
                    match client.sync_tasks(&db_id, &tasks) {
                        Ok(result) => {
                            let _ = tx.send(result.to_string());
                        }
                        Err(e) => {
                            let _ = tx.send(format!("Notion sync failed: {e}"));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(format!("Notion DB creation failed: {e}"));
                }
            }
        });
    }

    fn notion_status(&mut self) {
        let config = icebox_runtime::IceboxConfig::load();
        let config_api_key = config
            .notion
            .as_ref()
            .and_then(|n| n.api_key.as_deref())
            .map(String::from);

        let has_env_key = std::env::var("NOTION_API_KEY")
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        let has_config_key = config_api_key.is_some();

        let key_source = if has_env_key {
            Some("env (NOTION_API_KEY)")
        } else if has_config_key {
            Some("config.json")
        } else {
            None
        };

        let key_line = match key_source {
            Some(source) => {
                match icebox_tools::notion::NotionClient::from_env(config_api_key.as_deref()) {
                    Ok(client) => match client.verify_key() {
                        Ok(name) => format!("  API Key: {source} — valid (bot: {name})"),
                        Err(e) => format!("  API Key: {source} — invalid ({e})"),
                    },
                    Err(e) => format!("  API Key: {source} — error ({e})"),
                }
            }
            None => "  API Key: not configured".to_string(),
        };

        let db_line = match config.notion {
            Some(ref n) if n.database_id.is_some() => {
                format!(
                    "  Database: {}\n  Parent Page: {}",
                    n.database_id.as_deref().unwrap_or("-"),
                    n.parent_page_id.as_deref().unwrap_or("-"),
                )
            }
            _ => "  Database: not configured".to_string(),
        };

        let content = format!("Notion status:\n{key_line}\n{db_line}");
        let chat = self.active_chat_messages();
        chat.push(SidebarMessage {
            role: MessageRole::System,
            content,
        });
    }

    fn notion_reset(&mut self) {
        let mut config = icebox_runtime::IceboxConfig::load();
        config.notion = None;
        if let Err(e) = config.save() {
            self.set_status(format!("Failed to reset config: {e}"), true);
            return;
        }
        self.notion_pages_cache.clear();
        let chat = self.active_chat_messages();
        chat.push(SidebarMessage {
            role: MessageRole::System,
            content: "Notion configuration cleared.".into(),
        });
    }

    fn active_chat_messages(&mut self) -> &mut Vec<SidebarMessage> {
        if self.bottom_chat_focused {
            &mut self.bottom_chat.messages
        } else {
            &mut self.sidebar.messages
        }
    }

    fn drain_notify_events(&mut self) {
        let rx = match &self.notify_rx {
            Some(rx) => rx,
            None => return,
        };
        let mut messages = Vec::new();
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            // Worker thread finished — clear busy state
            self.notion_busy = None;
            self.notify_rx = None;
            if self.status_message.as_ref().is_some_and(|s| !s.is_error) {
                self.status_message = None;
            }
        }
        for msg in messages {
            // Handle page cache updates from notion search
            if let Some(json_str) = msg.strip_prefix("__PAGES__:") {
                if let Ok(pairs) = serde_json::from_str::<Vec<(String, String)>>(json_str) {
                    self.notion_pages_cache = pairs
                        .into_iter()
                        .map(|(id, title)| NotionPage { id, title })
                        .collect();
                }
                continue;
            }

            // Handle reload signal (e.g. after /notion pull)
            if msg == "__RELOAD__" {
                self.reload_tasks();
                continue;
            }

            let chat = if self.bottom_chat_focused {
                &mut self.bottom_chat.messages
            } else {
                &mut self.sidebar.messages
            };
            chat.push(SidebarMessage {
                role: MessageRole::System,
                content: msg,
            });
        }
    }

    fn handle_slash_command(&mut self, cmd: icebox_commands::SlashCommand) {
        match cmd {
            icebox_commands::SlashCommand::Help => {
                let help = icebox_commands::render_help();
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: help,
                });
            }
            icebox_commands::SlashCommand::Clear => {
                self.sidebar.messages.clear();
                // Also clear the runtime session
                let session_id = if self.bottom_chat_focused {
                    None
                } else {
                    self.active_task_id.clone()
                };
                if let Some(sender) = &self.ai_sender {
                    sender(RuntimeCommand::ClearSession { session_id });
                }
                self.set_status("Chat cleared", false);
            }
            icebox_commands::SlashCommand::Compact => {
                let session_id = if self.bottom_chat_focused {
                    None
                } else {
                    self.active_task_id.clone()
                };
                if let Some(sender) = &self.ai_sender {
                    sender(RuntimeCommand::CompactSession { session_id });
                }
                // Keep only recent UI messages
                let chat = if self.bottom_chat_focused {
                    &mut self.bottom_chat.messages
                } else {
                    &mut self.sidebar.messages
                };
                let len = chat.len();
                if len > 4 {
                    let recent = chat.split_off(len - 4);
                    *chat = recent;
                }
                self.set_status("Conversation compacted", false);
            }
            icebox_commands::SlashCommand::Status => {
                let task_count: usize = self.board.tasks.values().map(|v| v.len()).sum();
                let ai_status = if self.ai_sender.is_some() {
                    "connected"
                } else {
                    "not connected"
                };
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Tasks: {task_count} | AI: {ai_status} | Busy: {}",
                        self.ai_busy
                    ),
                });
            }
            icebox_commands::SlashCommand::Model { model } => {
                match model {
                    Some(name) => {
                        // Direct model switch by alias or ID
                        match icebox_runtime::resolve_model(&name) {
                            Some(m) => {
                                self.current_model = m.id.to_string();
                                let _ = icebox_runtime::IceboxConfig::save_model(m.id);
                                if let Some(sender) = &self.ai_sender {
                                    sender(RuntimeCommand::SwitchModel(m.id.to_string()));
                                }
                                self.sidebar.messages.push(SidebarMessage {
                                    role: MessageRole::System,
                                    content: format!(
                                        "Switched to {} ({}, {}K tokens, ${:.0}→${:.0}/M)",
                                        m.alias,
                                        m.id,
                                        m.max_tokens / 1000,
                                        m.input_per_million,
                                        m.output_per_million,
                                    ),
                                });
                            }
                            None => {
                                self.sidebar.messages.push(SidebarMessage {
                                    role: MessageRole::System,
                                    content: format!(
                                        "Unknown model: {name}\n{}",
                                        icebox_runtime::format_model_list(&self.current_model),
                                    ),
                                });
                            }
                        }
                    }
                    None => {
                        // Open interactive model selector
                        self.model_select_idx = icebox_runtime::MODELS
                            .iter()
                            .position(|m| m.id == self.current_model)
                            .unwrap_or(0);
                        self.mode = AppMode::SelectModel;
                    }
                }
            }
            icebox_commands::SlashCommand::Login => {
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: "OAuth login must be run from terminal.\nRun: icebox login\n\nOr set ANTHROPIC_API_KEY environment variable.".into(),
                });
            }
            icebox_commands::SlashCommand::Logout => {
                match icebox_runtime::clear_oauth_credentials() {
                    Ok(()) => {
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: "Logged out. OAuth credentials cleared.\nRestart icebox to use new credentials.".into(),
                        });
                        self.set_status("Logged out", false);
                    }
                    Err(e) => {
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: format!("Logout failed: {e}"),
                        });
                    }
                }
            }
            icebox_commands::SlashCommand::Cost => {
                let usage_info = format!(
                    "Session usage tracking not yet available.\nModel: {}",
                    self.current_model
                );
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: usage_info,
                });
            }
            icebox_commands::SlashCommand::Remember { text } => {
                let content = text.unwrap_or_default();
                if content.is_empty() {
                    self.sidebar.messages.push(SidebarMessage {
                        role: MessageRole::System,
                        content: "Usage: /remember <text>".into(),
                    });
                } else {
                    let source = self
                        .active_task_id
                        .clone()
                        .unwrap_or_else(|| "global".into());
                    if let Some(store) = &self.memory_store {
                        match store.add(content.clone(), source) {
                            Ok(_) => {
                                self.sidebar.messages.push(SidebarMessage {
                                    role: MessageRole::System,
                                    content: format!("Saved: {content}"),
                                });
                            }
                            Err(e) => {
                                self.sidebar.messages.push(SidebarMessage {
                                    role: MessageRole::System,
                                    content: format!("Failed to save memory: {e}"),
                                });
                            }
                        }
                    }
                }
            }
            icebox_commands::SlashCommand::Memory => {
                self.active_tab = Tab::Memory;
                self.mode = AppMode::Memory;
                self.reload_memory();
            }
            icebox_commands::SlashCommand::New { title } => {
                let title = title.unwrap_or_default();
                if title.is_empty() {
                    self.sidebar.messages.push(SidebarMessage {
                        role: MessageRole::System,
                        content: "Usage: /new <title>".into(),
                    });
                } else {
                    let task = Task::new(title.clone(), Column::Icebox, Priority::Medium);
                    let id: String = task.id.chars().take(8).collect();
                    match self.store.save(&task) {
                        Ok(()) => {
                            self.reload_tasks();
                            self.sidebar.messages.push(SidebarMessage {
                                role: MessageRole::System,
                                content: format!("Created: {title} [{id}]"),
                            });
                        }
                        Err(e) => {
                            self.sidebar.messages.push(SidebarMessage {
                                role: MessageRole::System,
                                content: format!("Failed to create task: {e}"),
                            });
                        }
                    }
                }
            }
            icebox_commands::SlashCommand::Move { column } => {
                let col_str = column.unwrap_or_default();
                let target = match col_str.as_str() {
                    "icebox" => Some(Column::Icebox),
                    "emergency" => Some(Column::Emergency),
                    "inprogress" | "in-progress" | "wip" => Some(Column::InProgress),
                    "testing" | "test" => Some(Column::Testing),
                    "complete" | "done" => Some(Column::Complete),
                    _ => None,
                };
                match target {
                    None => {
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: "Usage: /move <icebox|emergency|inprogress|testing|complete>".into(),
                        });
                    }
                    Some(col) => {
                        if let Some(task) = self.board.selected_task().cloned() {
                            let mut updated = task;
                            updated.column = col;
                            updated.updated_at = Utc::now();
                            match self.store.save(&updated) {
                                Ok(()) => {
                                    self.reload_tasks();
                                    self.set_status(
                                        format!("Moved to {}", col.display_name()),
                                        false,
                                    );
                                }
                                Err(e) => self.set_status(format!("Move failed: {e}"), true),
                            }
                        } else {
                            self.set_status("No task selected", true);
                        }
                    }
                }
            }
            icebox_commands::SlashCommand::Delete { id } => {
                let prefix = id.unwrap_or_default();
                if prefix.is_empty() {
                    if let Some(task) = self.board.selected_task() {
                        let task_id = task.id.clone();
                        let title = task.title.clone();
                        match self.store.delete(&task_id) {
                            Ok(()) => {
                                self.reload_tasks();
                                self.set_status(format!("Deleted: {title}"), false);
                            }
                            Err(e) => self.set_status(format!("Delete failed: {e}"), true),
                        }
                    } else {
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: "Usage: /delete <task-id-prefix> or select a task first".into(),
                        });
                    }
                } else {
                    let tasks = self.store.list().unwrap_or_default();
                    let matching: Vec<&Task> = tasks.iter().filter(|t| t.id.starts_with(&prefix)).collect();
                    match matching.len() {
                        0 => self.set_status(format!("No task matching '{prefix}'"), true),
                        1 => {
                            let title = matching[0].title.clone();
                            let task_id = matching[0].id.clone();
                            match self.store.delete(&task_id) {
                                Ok(()) => {
                                    self.reload_tasks();
                                    self.set_status(format!("Deleted: {title}"), false);
                                }
                                Err(e) => self.set_status(format!("Delete failed: {e}"), true),
                            }
                        }
                        n => self.set_status(format!("Ambiguous: {n} tasks match '{prefix}'"), true),
                    }
                }
            }
            icebox_commands::SlashCommand::Search { query } => {
                let q = query.unwrap_or_default().to_lowercase();
                if q.is_empty() {
                    self.sidebar.messages.push(SidebarMessage {
                        role: MessageRole::System,
                        content: "Usage: /search <query>".into(),
                    });
                } else {
                    let tasks = self.store.list().unwrap_or_default();
                    let results: Vec<&Task> = tasks
                        .iter()
                        .filter(|t| t.title.to_lowercase().contains(&q) || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q)))
                        .collect();
                    if results.is_empty() {
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: format!("No tasks matching '{q}'"),
                        });
                    } else {
                        let mut output = format!("Found {} task(s):\n", results.len());
                        for t in &results {
                            let short_id: String = t.id.chars().take(8).collect();
                            output.push_str(&format!(
                                "  [{short_id}] {} ({})\n",
                                t.title,
                                t.column.display_name()
                            ));
                        }
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: output,
                        });
                    }
                }
            }
            icebox_commands::SlashCommand::Export => {
                let tasks = self.store.list().unwrap_or_default();
                let mut output = String::from("# Icebox Board Export\n");
                for col in Column::ALL {
                    output.push_str(&format!("\n## {}\n", col.display_name()));
                    let col_tasks: Vec<&Task> = tasks.iter().filter(|t| t.column == col).collect();
                    if col_tasks.is_empty() {
                        output.push_str("_(empty)_\n");
                    } else {
                        for t in &col_tasks {
                            output.push_str(&format!("- [{}] {} ({})\n",
                                t.priority.label(), t.title,
                                t.tags.join(", ")
                            ));
                        }
                    }
                }
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: output,
                });
            }
            icebox_commands::SlashCommand::Diff => {
                match std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .current_dir(&self.workspace_path)
                    .output()
                {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let content = if stdout.is_empty() {
                            "No changes.".to_string()
                        } else {
                            stdout.into_owned()
                        };
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content,
                        });
                    }
                    Err(e) => {
                        self.sidebar.messages.push(SidebarMessage {
                            role: MessageRole::System,
                            content: format!("git diff failed: {e}"),
                        });
                    }
                }
            }
            icebox_commands::SlashCommand::Notion { action } => {
                self.handle_notion_command(action);
            }
            icebox_commands::SlashCommand::Resume { .. } => {
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: "Session resume not yet implemented.".into(),
                });
            }
            icebox_commands::SlashCommand::Unknown(name) => {
                self.sidebar.messages.push(SidebarMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Unknown command: /{name}. Type /help for available commands."
                    ),
                });
            }
        }
    }
}
