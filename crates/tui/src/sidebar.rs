use crate::theme;
use icebox_task::model::Task;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap};

#[derive(Default, Clone)]
pub struct TextSelection {
    pub start: (u16, u16), // (x, y) terminal coordinates
    pub end: (u16, u16),   // (x, y) terminal coordinates
    pub active: bool,
}

#[derive(Default)]
pub struct SidebarState {
    pub input: String,
    pub cursor_pos: usize,
    pub messages: Vec<SidebarMessage>,
    pub detail_scroll: u16,
    pub chat_scroll: u16,
    pub chat_focused: bool,
    pub text_selection: Option<TextSelection>,
    pub rendered_text_lines: Vec<String>,
    pub rendered_chat_lines: Vec<String>,
    pub detail_rect: Option<Rect>,
    pub chat_rect: Option<Rect>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub input_stash: String,
}

#[derive(Clone)]
pub struct SidebarMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl SidebarState {
    pub fn insert_char(&mut self, c: char) {
        if self.cursor_pos > self.input.len() {
            self.cursor_pos = self.input.len();
        }
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let prev = self.input[..self.cursor_pos]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
        self.input.drain(prev..self.cursor_pos);
        self.cursor_pos = prev;
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        self.cursor_pos = self.input[..self.cursor_pos]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos >= self.input.len() {
            return;
        }
        self.cursor_pos = self.input[self.cursor_pos..]
            .char_indices()
            .nth(1)
            .map_or(self.input.len(), |(i, _)| self.cursor_pos + i);
    }

    pub fn take_input(&mut self) -> String {
        self.cursor_pos = 0;
        let input = std::mem::take(&mut self.input);
        if !input.trim().is_empty() {
            self.input_history.push(input.clone());
        }
        self.history_index = None;
        self.input_stash.clear();
        input
    }

    pub fn history_up(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.input_stash = self.input.clone();
                self.history_index = Some(self.input_history.len().saturating_sub(1));
            }
            Some(0) => return,
            Some(ref mut idx) => *idx = idx.saturating_sub(1),
        }
        if let Some(idx) = self.history_index
            && let Some(entry) = self.input_history.get(idx)
        {
            self.input = entry.clone();
            self.cursor_pos = self.input.len();
        }
    }

    pub fn history_down(&mut self) {
        let Some(idx) = self.history_index else {
            return;
        };
        if idx >= self.input_history.len().saturating_sub(1) {
            self.history_index = None;
            self.input = std::mem::take(&mut self.input_stash);
            self.cursor_pos = self.input.len();
            return;
        }
        self.history_index = Some(idx + 1);
        if let Some(entry) = self.input_history.get(idx + 1) {
            self.input = entry.clone();
            self.cursor_pos = self.input.len();
        }
    }
}

pub fn render_sidebar(
    task: Option<&Task>,
    state: &mut SidebarState,
    sidebar_focused: bool,
    ai_busy: bool,
    spinner_tick: u16,
    area: Rect,
    buf: &mut Buffer,
) {
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::sidebar_border_style())
        .padding(Padding::new(1, 1, 0, 0));

    let inner = outer_block.inner(area);
    outer_block.render(area, buf);

    // Split into detail (top) and chat (bottom)
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let Some(detail_area) = sections.first().copied() else {
        return;
    };
    let Some(chat_area) = sections.get(1).copied() else {
        return;
    };

    // --- Detail section ---
    let detail_focused = !state.chat_focused && !sidebar_focused;
    let detail_border = if detail_focused {
        Style::default().fg(ratatui::style::Color::Cyan)
    } else {
        theme::dim_style()
    };
    let detail_block = Block::default()
        .title(" Detail ")
        .borders(Borders::ALL)
        .border_style(detail_border)
        .padding(Padding::uniform(1));
    let detail_inner = detail_block.inner(detail_area);
    detail_block.render(detail_area, buf);

    let mut detail_lines: Vec<Line<'_>> = Vec::new();
    match task {
        Some(task) => {
            render_task_detail(task, detail_inner.width, &mut detail_lines);
        }
        None => {
            detail_lines.push(Line::from(Span::styled(
                "No task selected",
                theme::dim_style(),
            )));
        }
    }

    state.rendered_text_lines = detail_lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();
    state.detail_rect = Some(detail_area);

    let detail_para = Paragraph::new(detail_lines)
        .wrap(Wrap { trim: false })
        .scroll((state.detail_scroll, 0));
    detail_para.render(detail_inner, buf);

    if let Some(sel) = &state.text_selection {
        apply_selection_highlight(sel, detail_inner, buf);
    }

    // --- Chat section ---
    let chat_focused_style = if state.chat_focused || sidebar_focused {
        Style::default().fg(ratatui::style::Color::Cyan)
    } else {
        theme::dim_style()
    };

    let chat_block = Block::default()
        .title(" AI Chat ")
        .borders(Borders::ALL)
        .border_style(chat_focused_style);
    let chat_inner = chat_block.inner(chat_area);
    chat_block.render(chat_area, buf);
    state.chat_rect = Some(chat_area);

    // Split chat inner into messages area + input area
    let chat_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(chat_inner);

    let Some(messages_area) = chat_layout.first().copied() else {
        return;
    };
    let Some(input_area) = chat_layout.get(1).copied() else {
        return;
    };

    // Render chat messages
    let mut chat_lines: Vec<Line<'_>> = Vec::new();
    if state.messages.is_empty() {
        chat_lines.push(Line::from(Span::styled(
            "Type a message to chat with AI...",
            theme::dim_style(),
        )));
    } else {
        for msg in &state.messages {
            match msg.role {
                MessageRole::User => {
                    chat_lines.push(Line::from(vec![
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
                    render_markdown_lines(&msg.content, &mut chat_lines);
                }
                MessageRole::System => {
                    let line = render_system_message_line(&msg.content);
                    chat_lines.push(line);
                }
            }
        }
    }

    if ai_busy {
        let spinner = crate::app::SPINNER_FRAMES
            .get((spinner_tick as usize / 3) % crate::app::SPINNER_FRAMES.len())
            .copied()
            .unwrap_or('⠋');
        chat_lines.push(Line::from(vec![
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

    state.rendered_chat_lines = chat_lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();

    let chat_para = Paragraph::new(chat_lines)
        .wrap(Wrap { trim: false })
        .scroll((state.chat_scroll, 0));
    chat_para.render(messages_area, buf);

    if let Some(sel) = &state.text_selection {
        apply_selection_highlight(sel, messages_area, buf);
    }

    // Render input
    render_input_area(state, input_area, buf);
}

fn normalize_selection(sel: &TextSelection) -> ((u16, u16), (u16, u16)) {
    if sel.start.1 < sel.end.1 || (sel.start.1 == sel.end.1 && sel.start.0 <= sel.end.0) {
        (sel.start, sel.end)
    } else {
        (sel.end, sel.start)
    }
}

fn apply_selection_highlight(sel: &TextSelection, area: Rect, buf: &mut Buffer) {
    let (start, end) = normalize_selection(sel);

    for y in start.1..=end.1 {
        if y < area.y || y >= area.y.saturating_add(area.height) {
            continue;
        }
        let x_start = if y == start.1 {
            start.0.max(area.x)
        } else {
            area.x
        };
        let x_end = if y == end.1 {
            end.0.min(area.x.saturating_add(area.width))
        } else {
            area.x.saturating_add(area.width)
        };

        for x in x_start..x_end {
            if let Some(cell) = buf.cell_mut(Position { x, y }) {
                cell.set_style(
                    Style::default()
                        .bg(ratatui::style::Color::White)
                        .fg(ratatui::style::Color::Black),
                );
            }
        }
    }
}

fn render_task_detail<'a>(task: &'a Task, width: u16, lines: &mut Vec<Line<'a>>) {
    lines.push(Line::from(vec![Span::styled(
        &task.title,
        Style::default().add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![
        Span::styled("Status: ", theme::dim_style()),
        Span::styled(
            task.column.display_name(),
            theme::column_style(task.column, false),
        ),
        Span::raw("  "),
        Span::styled("Priority: ", theme::dim_style()),
        Span::styled(
            format!("{} {}", task.priority.symbol(), task.priority.label()),
            theme::priority_style(task.priority),
        ),
    ]));

    if !task.tags.is_empty() {
        let tags = task.tags.join(", ");
        lines.push(Line::from(vec![
            Span::styled("Tags: ", theme::dim_style()),
            Span::raw(tags),
        ]));
    }

    // Swimlane
    if let Some(ref lane) = task.swimlane {
        lines.push(Line::from(vec![
            Span::styled("Swimlane: ", theme::dim_style()),
            Span::styled(
                lane.as_str(),
                Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    // Dependencies
    if !task.depends_on.is_empty() {
        let deps = task.depends_on.join(", ");
        lines.push(Line::from(vec![
            Span::styled("Depends: ", theme::dim_style()),
            Span::raw(deps),
        ]));
    }

    // Progress
    if let Some(progress) = &task.progress {
        lines.push(Line::from(vec![
            Span::styled("Progress: ", theme::dim_style()),
            Span::raw(format!("{} ({}%)", progress.display(), progress.percent())),
        ]));
    }

    // Dates
    if task.start_date.is_some() || task.due_date.is_some() {
        let mut date_spans = Vec::new();
        if let Some(start) = &task.start_date {
            date_spans.push(Span::styled("Start: ", theme::dim_style()));
            date_spans.push(Span::raw(start.format("%Y-%m-%d").to_string()));
            if task.due_date.is_some() {
                date_spans.push(Span::raw("  "));
            }
        }
        if let Some(due) = &task.due_date {
            date_spans.push(Span::styled("Due: ", theme::dim_style()));
            date_spans.push(Span::raw(due.format("%Y-%m-%d").to_string()));
        }
        lines.push(Line::from(date_spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "─".repeat(width.saturating_sub(1) as usize),
        theme::dim_style(),
    )]));
    lines.push(Line::from(""));

    // Render body, stripping sections whose content is only link references
    // (these are rendered as Notion-style icon blocks below)
    let body_lines: Vec<&str> = task.body.lines().collect();
    let mut i = 0;
    while i < body_lines.len() {
        let trimmed = body_lines[i].trim().to_lowercase();
        // Detect ## header that might be a references section
        if trimmed.starts_with("## ") {
            // Look ahead: if all non-empty lines are link patterns, skip the section
            let section_start = i;
            i += 1;
            let mut all_links = true;
            let mut has_content = false;
            while i < body_lines.len() {
                let lt = body_lines[i].trim();
                if lt.is_empty() {
                    i += 1;
                    continue;
                }
                // Next ## header — stop
                if lt.starts_with("## ") {
                    break;
                }
                has_content = true;
                let ll = lt.to_lowercase();
                let is_link_line = ll.starts_with("- commit:")
                    || ll.starts_with("- branch:")
                    || ll.starts_with("- pr")
                    || ll.starts_with("- issue")
                    || ll.starts_with("- #")
                    || ll.starts_with("- http")
                    || ll.starts_with("commit:")
                    || ll.starts_with("branch:")
                    || ll.starts_with("pr#")
                    || ll.starts_with("pr ")
                    || ll.starts_with("issue#")
                    || ll.starts_with("issue ")
                    || ll.starts_with('#') && ll.len() > 1 && ll.as_bytes().get(1).is_some_and(|b| b.is_ascii_digit());
                if !is_link_line {
                    all_links = false;
                    break;
                }
                i += 1;
            }
            if all_links && has_content {
                // Skip entire section — link blocks will render it
                continue;
            }
            // Not a link section — render it normally
            for line in &body_lines[section_start..i] {
                lines.push(Line::from((*line).to_string()));
            }
            continue;
        }
        lines.push(Line::from(body_lines[i].to_string()));
        i += 1;
    }

    // Render link blocks (Notion-style icons)
    let parsed_links = icebox_task::links::parse_links(&task.body);
    if !parsed_links.is_empty() {
        lines.push(Line::from(""));
        for link in &parsed_links {
            let block_line = render_link_block(link);
            lines.push(block_line);
        }
    }
}

/// Render a single link as a Notion-style inline block:
///   ⎇ branch:feature/auth │ ● issue#42 │ ↳ PR#123 │ ○ commit:abc1234
fn render_link_block(link: &icebox_task::links::TaskLink) -> Line<'static> {
    use icebox_task::links::LinkKind;
    use ratatui::style::Color;

    let (icon_color, bg_color) = match link.kind {
        LinkKind::Commit => (Color::Yellow, Color::Rgb(40, 35, 20)),
        LinkKind::PR => (Color::Green, Color::Rgb(20, 40, 25)),
        LinkKind::Issue => (Color::Red, Color::Rgb(40, 20, 20)),
        LinkKind::Branch => (Color::Cyan, Color::Rgb(20, 35, 40)),
        LinkKind::Url => (Color::Blue, Color::Rgb(20, 20, 40)),
    };

    let icon = link.kind.icon();
    let prefix = link.kind.label_prefix();
    let display_url = match &link.url {
        Some(url) => {
            let short: String = url.chars().take(40).collect();
            if url.len() > 40 {
                format!(" {short}...")
            } else {
                format!(" {short}")
            }
        }
        None => String::new(),
    };

    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            format!(" {icon} "),
            Style::default().fg(icon_color).bg(bg_color),
        ),
        Span::styled(
            format!(" {prefix}: "),
            Style::default().fg(Color::DarkGray).bg(bg_color),
        ),
        Span::styled(
            link.label.clone(),
            Style::default()
                .fg(Color::White)
                .bg(bg_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display_url,
            Style::default().fg(Color::DarkGray).bg(bg_color),
        ),
        Span::styled(" ", Style::default().bg(bg_color)),
    ])
}

/// Render markdown content as styled terminal lines.
pub fn render_markdown_lines<'a>(content: &str, lines: &mut Vec<Line<'a>>) {
    use ratatui::style::Color;

    let mut in_code_block = false;

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();

        // Code block toggle
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                let lang = trimmed.strip_prefix("```").unwrap_or_default();
                if lang.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  ┌─────────────",
                        Style::default().fg(Color::DarkGray),
                    )));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  ┌─ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            lang.to_string(),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(" ─────", Style::default().fg(Color::DarkGray)),
                    ]));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "  └─────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            continue;
        }

        // Inside code block
        if in_code_block {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    raw_line.to_string(),
                    Style::default().fg(Color::Green),
                ),
            ]));
            continue;
        }

        // Headers
        if let Some(rest) = trimmed.strip_prefix("#### ") {
            lines.push(Line::from(Span::styled(
                format!("  {rest}"),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                format!("  {rest}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("  {rest}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!("  {rest}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            lines.push(Line::from(Span::styled(
                "  ─────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        // List items
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let rest = &trimmed[2..];
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
            ];
            render_inline_markdown(rest, &mut spans);
            lines.push(Line::from(spans));
            continue;
        }

        // Numbered list
        if let Some(pos) = trimmed.find(". ")
            && pos <= 3
            && trimmed[..pos].chars().all(|c| c.is_ascii_digit())
        {
            let rest = &trimmed[pos + 2..];
            let num = &trimmed[..pos + 1];
            let mut spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{num} "),
                    Style::default().fg(Color::Cyan),
                ),
            ];
            render_inline_markdown(rest, &mut spans);
            lines.push(Line::from(spans));
            continue;
        }

        // Table rows
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            if trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ') {
                // Separator row
                lines.push(Line::from(Span::styled(
                    format!("  {trimmed}"),
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("  {trimmed}"),
                    Style::default().fg(Color::White),
                )));
            }
            continue;
        }

        // Empty line
        if trimmed.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // Regular text with inline markdown
        let mut spans = vec![Span::styled("  ", Style::default())];
        render_inline_markdown(trimmed, &mut spans);
        lines.push(Line::from(spans));
    }
}

/// Parse inline markdown: **bold**, `code`, *italic*
fn render_inline_markdown<'a>(text: &str, spans: &mut Vec<Span<'a>>) {
    use ratatui::style::Color;

    let mut remaining = text;

    while !remaining.is_empty() {
        // **bold**
        if let Some(start) = remaining.find("**") {
            if start > 0 {
                spans.push(Span::styled(
                    remaining[..start].to_string(),
                    Style::default().fg(Color::White),
                ));
            }
            let after = &remaining[start + 2..];
            if let Some(end) = after.find("**") {
                spans.push(Span::styled(
                    after[..end].to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
                remaining = &after[end + 2..];
                continue;
            }
            spans.push(Span::styled(
                remaining[start..].to_string(),
                Style::default().fg(Color::White),
            ));
            return;
        }

        // `code`
        if let Some(start) = remaining.find('`') {
            if start > 0 {
                spans.push(Span::styled(
                    remaining[..start].to_string(),
                    Style::default().fg(Color::White),
                ));
            }
            let after = &remaining[start + 1..];
            if let Some(end) = after.find('`') {
                spans.push(Span::styled(
                    after[..end].to_string(),
                    Style::default()
                        .fg(Color::Green)
                        .bg(Color::Rgb(30, 30, 30)),
                ));
                remaining = &after[end + 1..];
                continue;
            }
            spans.push(Span::styled(
                remaining[start..].to_string(),
                Style::default().fg(Color::White),
            ));
            return;
        }

        // Plain text
        spans.push(Span::styled(
            remaining.to_string(),
            Style::default().fg(Color::White),
        ));
        return;
    }
}

/// Render a system message with color-coded ⏺ indicator.
/// - `[tool: bash] ...` → green ⏺ (bash execution)
/// - `[tool: ...]` → yellow ⏺ (other tool call)
/// - `[... result]` / `[... ERROR]` → dim result line
/// - `Error: ...` → red ⏺
/// - Other → dim
pub fn render_system_message_line(content: &str) -> Line<'static> {
    use ratatui::style::Color;

    if content.starts_with("[tool: bash]") {
        let detail = content.strip_prefix("[tool: bash]").unwrap_or_default().trim();
        Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(Color::Green)),
            Span::styled("bash ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(
                truncate_display(detail, 80),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else if content.starts_with("[tool: ") {
        let inner = content
            .strip_prefix("[tool: ")
            .and_then(|s| s.split_once(']'))
            .map_or((content.to_string(), String::new()), |(name, rest)| {
                (name.to_string(), rest.trim().to_string())
            });
        Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{} ", inner.0),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                truncate_display(&inner.1, 80),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else if content.contains(" result]") || content.contains(" ERROR]") {
        let is_error = content.contains("ERROR]");
        let color = if is_error { Color::Red } else { Color::DarkGray };
        // Extract just the output part after the bracket
        let output = content
            .find(']')
            .map(|i| content.get(i + 1..).unwrap_or_default().trim())
            .unwrap_or(content);
        let prefix_end = content.find(']').unwrap_or(0) + 1;
        let prefix: String = content.chars().take(prefix_end).collect();
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                truncate_display(&prefix, 25),
                Style::default().fg(color),
            ),
            Span::raw(" "),
            Span::styled(
                truncate_display(output, 60),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else if content.starts_with("Error:") {
        Line::from(vec![
            Span::styled("⏺ ", Style::default().fg(Color::Red)),
            Span::styled(
                content.to_string(),
                Style::default().fg(Color::Red),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(content.to_string(), theme::dim_style()),
        ])
    }
}

fn truncate_display(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{t}...")
    }
}

fn render_input_area(state: &SidebarState, area: Rect, buf: &mut Buffer) {
    let input_block = Block::default()
        .title(" Input (Enter to send) ")
        .borders(Borders::ALL)
        .border_style(theme::dim_style());
    let input_inner = input_block.inner(area);
    input_block.render(area, buf);

    let input_text = Line::from(Span::styled(&state.input, theme::input_style()));
    buf.set_line(input_inner.x, input_inner.y, &input_text, input_inner.width);
}
