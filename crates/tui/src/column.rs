use crate::card;
use crate::theme;
use icebox_task::model::{Column, Task};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, StatefulWidget, Widget};

pub struct ColumnWidget<'a> {
    pub column: Column,
    pub tasks: &'a [Task],
    pub focused: bool,
    pub selected_task: Option<usize>,
}

#[derive(Default)]
pub struct ColumnState {
    pub scroll_offset: usize,
}

impl<'a> StatefulWidget for ColumnWidget<'a> {
    type State = ColumnState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let title_style = theme::column_style(self.column, self.focused);
        let border_color = if self.focused {
            theme::column_color(self.column)
        } else {
            ratatui::style::Color::DarkGray
        };

        let count = self.tasks.len();
        let title = format!(" {} ({count}) ", self.column.display_name());

        let block = Block::default()
            .title(Line::from(Span::styled(title, title_style)))
            .borders(Borders::ALL)
            .border_style(ratatui::style::Style::default().fg(border_color))
            .padding(Padding::horizontal(1));

        let inner = block.inner(area);
        block.render(area, buf);

        if self.tasks.is_empty() {
            let empty_msg = Line::from(Span::styled("(empty)", theme::dim_style()));
            if inner.height > 0 {
                buf.set_line(inner.x, inner.y, &empty_msg, inner.width);
            }
            return;
        }

        let max_visible = inner.height as usize;
        if state.scroll_offset >= count {
            state.scroll_offset = count.saturating_sub(1);
        }

        if let Some(sel) = self.selected_task {
            if sel < state.scroll_offset {
                state.scroll_offset = sel;
            }
            if sel >= state.scroll_offset + max_visible / 2 {
                state.scroll_offset = sel.saturating_sub(max_visible / 2);
            }
        }

        let mut y = inner.y;
        for (i, task) in self.tasks.iter().enumerate().skip(state.scroll_offset) {
            if y >= inner.y + inner.height {
                break;
            }
            let selected = self.selected_task == Some(i);
            let lines = card::render_card(task, selected, inner.width);
            for line in &lines {
                if y >= inner.y + inner.height {
                    break;
                }
                buf.set_line(inner.x, y, line, inner.width);
                y += 1;
            }
            // separator between cards
            if y < inner.y + inner.height {
                y += 1;
            }
        }
    }
}
