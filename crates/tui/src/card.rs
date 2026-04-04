use crate::theme;
use icebox_task::model::Task;
use ratatui::text::{Line, Span};

/// Truncate a string to fit within `max_chars` characters, adding "..." if truncated.
/// Safe for multi-byte UTF-8 strings.
fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{truncated}...")
}

/// Whether the card has a metadata line (tags, progress, or dates).
fn has_meta_line(task: &Task) -> bool {
    !task.tags.is_empty()
        || task.progress.is_some()
        || task.start_date.is_some()
        || task.due_date.is_some()
}

/// Number of lines a card occupies (excluding separator).
/// Must match `render_card` logic: 1 line for title, +1 if metadata present.
pub fn card_line_count(task: &Task, _width: u16) -> usize {
    if has_meta_line(task) { 2 } else { 1 }
}

pub fn render_card(task: &Task, selected: bool, width: u16) -> Vec<Line<'static>> {
    let style = theme::card_style(selected);
    let priority_style = theme::priority_style(task.priority);
    let w = width.saturating_sub(4) as usize;

    let title = truncate_str(&task.title, w);

    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{} ", task.priority.symbol()), priority_style),
        Span::styled(title, style),
    ])];

    if has_meta_line(task) {
        let mut parts: Vec<String> = Vec::new();

        // Tags (up to 3)
        if !task.tags.is_empty() {
            let tags_str: String = task
                .tags
                .iter()
                .take(3)
                .map(|t| format!("[{t}]"))
                .collect::<Vec<_>>()
                .join(" ");
            parts.push(tags_str);
        }

        // Progress (e.g., "3/10")
        if let Some(progress) = &task.progress {
            parts.push(format!("[{}]", progress.display()));
        }

        // Start date (e.g., "▸04/03")
        if let Some(start) = &task.start_date {
            parts.push(format!("▸{}", start.format("%m/%d")));
        }

        // Due date (e.g., "~04/15")
        if let Some(due) = &task.due_date {
            parts.push(format!("~{}", due.format("%m/%d")));
        }

        let meta = truncate_str(&parts.join(" "), w);
        lines.push(Line::from(Span::styled(
            format!("  {meta}"),
            theme::dim_style(),
        )));
    }

    lines
}
