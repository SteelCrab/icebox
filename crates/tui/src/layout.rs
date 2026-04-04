use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub header: Rect,
    pub tab_bar: Rect,
    pub columns: Vec<Rect>,
    pub sidebar: Option<Rect>,
    pub bottom_chat: Option<Rect>,
    pub status_bar: Rect,
}

pub fn compute_layout(
    area: Rect,
    sidebar_open: bool,
    bottom_chat_open: bool,
    bottom_chat_height: u16,
) -> AppLayout {
    // Vertical: header | tab_bar | main | (bottom_chat) | status_bar
    let mut v_constraints = vec![
        Constraint::Length(1), // header
        Constraint::Length(1), // tab bar
    ];

    if bottom_chat_open {
        v_constraints.push(Constraint::Min(6)); // board (shrink to make room)
        v_constraints.push(Constraint::Length(bottom_chat_height)); // bottom chat
    } else {
        v_constraints.push(Constraint::Min(0)); // board full
    }
    v_constraints.push(Constraint::Length(1)); // status bar

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(v_constraints)
        .split(area);

    let header = vertical[0];
    let tab_bar = vertical[1];
    let main_area = vertical[2];
    let (bottom_chat, status_bar) = if bottom_chat_open {
        (Some(vertical[3]), vertical[4])
    } else {
        (None, vertical[3])
    };

    // Horizontal: board columns | (sidebar)
    let (board_area, sidebar) = if sidebar_open {
        let h = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(main_area);
        (h[0], Some(h[1]))
    } else {
        (main_area, None)
    };

    let col_constraints: Vec<Constraint> = (0..5).map(|_| Constraint::Ratio(1, 5)).collect();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints)
        .split(board_area)
        .to_vec();

    AppLayout {
        header,
        tab_bar,
        columns,
        sidebar,
        bottom_chat,
        status_bar,
    }
}
