use icebox_task::model::{Column, Priority};
use ratatui::style::{Color, Modifier, Style};

pub fn column_color(column: Column) -> Color {
    match column {
        Column::Icebox => Color::Cyan,
        Column::Emergency => Color::Red,
        Column::InProgress => Color::Yellow,
        Column::Testing => Color::Magenta,
        Column::Complete => Color::Green,
    }
}

pub fn column_style(column: Column, focused: bool) -> Style {
    let color = column_color(column);
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

pub fn priority_color(priority: Priority) -> Color {
    match priority {
        Priority::Low => Color::Green,
        Priority::Medium => Color::Yellow,
        Priority::High => Color::Rgb(255, 165, 0),
        Priority::Critical => Color::Red,
    }
}

pub fn priority_style(priority: Priority) -> Style {
    Style::default().fg(priority_color(priority))
}

pub fn card_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

pub fn sidebar_border_style() -> Style {
    Style::default().fg(Color::Blue)
}

pub fn header_style() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn dim_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn input_style() -> Style {
    Style::default().fg(Color::White)
}

pub fn status_bar_style() -> Style {
    Style::default().bg(Color::DarkGray).fg(Color::White)
}
