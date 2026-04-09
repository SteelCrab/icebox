use crate::app::{App, AppMode, DragTarget, EditField, Tab};
use crate::card;
use crate::sidebar::TextSelection;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) => handle_key(app, key),
        Event::Mouse(mouse) => handle_mouse(app, mouse),
        Event::Resize(_, _) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    app.clear_status();

    // Tool approval modal takes priority
    if app.pending_tool_approval.is_some() {
        match key.code {
            KeyCode::Char('1') | KeyCode::Enter => {
                app.respond_tool_approval(icebox_runtime::ToolApproval::Yes);
            }
            KeyCode::Char('2') => {
                app.respond_tool_approval(icebox_runtime::ToolApproval::AlwaysYes);
            }
            KeyCode::Char('3') | KeyCode::Esc => {
                app.respond_tool_approval(icebox_runtime::ToolApproval::No);
            }
            _ => {}
        }
        return;
    }

    // Modal modes take priority over bottom chat focus
    match app.mode {
        AppMode::SelectModel => {
            handle_select_model_key(app, key);
            return;
        }
        AppMode::CreateTask => {
            handle_create_key(app, key);
            return;
        }
        AppMode::ConfirmDelete => {
            handle_confirm_delete_key(app, key);
            return;
        }
        AppMode::EditTask => {
            handle_edit_key(app, key);
            return;
        }
        _ => {}
    }

    // Bottom chat focused — handle input first
    if app.bottom_chat_focused {
        handle_bottom_chat_key(app, key);
        return;
    }

    // Tab switching (1 = Board, 2 = Memory) — only in base modes
    if matches!(app.mode, AppMode::Board | AppMode::Memory) {
        match key.code {
            KeyCode::Char('1') => {
                app.active_tab = Tab::Board;
                app.mode = AppMode::Board;
                return;
            }
            KeyCode::Char('2') => {
                app.active_tab = Tab::Memory;
                app.mode = AppMode::Memory;
                app.reload_memory();
                return;
            }
            _ => {}
        }
    }

    match app.mode {
        AppMode::Board => handle_board_key(app, key),
        AppMode::TaskDetail => handle_detail_key(app, key),
        AppMode::EditTask => handle_edit_key(app, key),
        AppMode::CreateTask => handle_create_key(app, key),
        AppMode::ConfirmDelete => handle_confirm_delete_key(app, key),
        AppMode::SelectModel => handle_select_model_key(app, key),
        AppMode::Memory => handle_memory_key(app, key),
    }
}

fn handle_board_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,

        KeyCode::Char('h') | KeyCode::Left => app.board.move_focus_left(),
        KeyCode::Char('l') | KeyCode::Right => app.board.move_focus_right(),
        KeyCode::Char('j') | KeyCode::Down => app.board.move_selection_down(),
        KeyCode::Char('k') | KeyCode::Up => app.board.move_selection_up(),

        KeyCode::Enter => {
            if let Some(task) = app.board.selected_task() {
                let task_id = task.id.clone();
                app.switch_sidebar_task(Some(task_id));
                app.mode = AppMode::TaskDetail;
            }
        }

        KeyCode::Char('n') => {
            app.create_input.clear();
            app.create_tags.clear();
            app.create_swimlane = app
                .board
                .active_swimlane_name()
                .unwrap_or_default()
                .to_string();
            app.create_start_date.clear();
            app.create_due_date.clear();
            app.create_field = crate::app::CreateField::Title;
            app.create_priority_idx = 1;
            app.mode = AppMode::CreateTask;
        }

        KeyCode::Char('L') | KeyCode::Char('>') => app.move_task_right(),
        KeyCode::Char('H') | KeyCode::Char('<') => app.move_task_left(),

        // Swimlane navigation
        KeyCode::Char(']') => app.board.next_swimlane(),
        KeyCode::Char('[') => app.board.prev_swimlane(),

        // Delete with 'd' or Enter on confirm
        KeyCode::Char('d') => {
            if app.board.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            }
        }

        KeyCode::Char('r') => app.reload_tasks(),

        // Toggle bottom chat
        KeyCode::Char('/') => {
            app.bottom_chat_open = !app.bottom_chat_open;
            app.bottom_chat_focused = app.bottom_chat_open;
        }

        _ => {}
    }
}

fn handle_detail_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Esc: layered unfocus — input → chat → close sidebar
        KeyCode::Esc => {
            if app.sidebar_focused {
                app.sidebar_focused = false;
            } else if app.sidebar.chat_focused {
                app.sidebar.chat_focused = false;
            } else {
                app.switch_sidebar_task(None);
                app.mode = AppMode::Board;
            }
        }

        // Sidebar input handlers (must precede unguarded Char arms)
        KeyCode::Char(c) if app.sidebar_focused => {
            app.sidebar.insert_char(c);
        }
        KeyCode::Backspace if app.sidebar_focused => {
            app.sidebar.delete_char();
        }
        KeyCode::Left if app.sidebar_focused => {
            app.sidebar.move_cursor_left();
        }
        KeyCode::Right if app.sidebar_focused => {
            app.sidebar.move_cursor_right();
        }
        KeyCode::Up if app.sidebar_focused => {
            app.sidebar.history_up();
        }
        KeyCode::Down if app.sidebar_focused => {
            app.sidebar.history_down();
        }
        KeyCode::Enter if app.sidebar_focused => {
            let input = app.sidebar.take_input();
            if !input.is_empty() {
                app.handle_sidebar_input(input);
            }
        }
        // Tab: autocomplete slash command in sidebar, or toggle focus
        KeyCode::Tab if app.sidebar_focused && app.sidebar.input.starts_with('/') => {
            if let Some(completed) = icebox_commands::autocomplete(&app.sidebar.input) {
                app.sidebar.input = format!("/{completed} ");
                app.sidebar.cursor_pos = app.sidebar.input.len();
            }
        }

        // 'q': close sidebar (only reached when sidebar_focused is false,
        // since Char(c) guard above catches 'q' while typing)
        KeyCode::Char('q') => {
            app.switch_sidebar_task(None);
            app.mode = AppMode::Board;
            app.sidebar_focused = false;
        }

        KeyCode::Tab | KeyCode::Char('i') => {
            // Cycle: detail scroll → chat scroll → input mode
            if !app.sidebar.chat_focused && !app.sidebar_focused {
                // detail → chat
                app.sidebar.chat_focused = true;
            } else if app.sidebar.chat_focused && !app.sidebar_focused {
                // chat → input
                app.sidebar.chat_focused = false;
                app.sidebar_focused = true;
            } else {
                // input → detail
                app.sidebar_focused = false;
                app.sidebar.chat_focused = false;
            }
        }

        KeyCode::Char('j') | KeyCode::Down if !app.sidebar_focused => {
            if app.sidebar.chat_focused {
                app.sidebar.chat_scroll = app.sidebar.chat_scroll.saturating_add(1);
            } else {
                app.sidebar.detail_scroll = app.sidebar.detail_scroll.saturating_add(1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up if !app.sidebar_focused => {
            if app.sidebar.chat_focused {
                app.sidebar.chat_scroll = app.sidebar.chat_scroll.saturating_sub(1);
            } else {
                app.sidebar.detail_scroll = app.sidebar.detail_scroll.saturating_sub(1);
            }
        }

        // Edit task
        KeyCode::Char('e') if !app.sidebar_focused => {
            app.start_edit_task();
        }

        KeyCode::Char('>') => app.move_task_right(),
        KeyCode::Char('<') => app.move_task_left(),

        // Clear swimlane from selected task
        KeyCode::Char('s') if !app.sidebar_focused => {
            if let Some(task) = app.board.selected_task().cloned() {
                if task.swimlane.is_some() {
                    let mut task = task;
                    task.swimlane = None;
                    task.updated_at = chrono::Utc::now();
                    match app.store.save(&task) {
                        Ok(()) => {
                            app.reload_tasks();
                            app.set_status("Swimlane cleared", false);
                        }
                        Err(e) => {
                            app.set_status(format!("Failed: {e}"), true);
                        }
                    }
                } else {
                    app.set_status("No swimlane to clear. Use /swimlane <name> to set", false);
                }
            }
        }

        // Toggle bottom chat from detail mode
        KeyCode::Char('/') if !app.sidebar_focused => {
            app.bottom_chat_open = !app.bottom_chat_open;
            if app.bottom_chat_open {
                app.bottom_chat_focused = true;
                app.sidebar_focused = false;
            } else {
                app.bottom_chat_focused = false;
            }
        }

        _ => {}
    }
}

// ── Bottom chat input handling ──

fn handle_bottom_chat_key(app: &mut App, key: KeyEvent) {
    // Ctrl+Up/Down to resize bottom chat
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Up => {
                app.bottom_chat_height = app.bottom_chat_height.saturating_add(3).min(40);
                return;
            }
            KeyCode::Down => {
                app.bottom_chat_height = app.bottom_chat_height.saturating_sub(3).max(6);
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => {
            app.bottom_chat_focused = false;
        }
        KeyCode::Enter => {
            let input = app.bottom_chat.take_input();
            if !input.is_empty() {
                app.handle_bottom_chat_input(input);
            }
        }
        // Tab: autocomplete slash command
        KeyCode::Tab => {
            if app.bottom_chat.input.starts_with('/')
                && let Some(completed) = icebox_commands::autocomplete(&app.bottom_chat.input)
            {
                app.bottom_chat.input = format!("/{completed} ");
                app.bottom_chat.cursor_pos = app.bottom_chat.input.len();
            }
        }
        KeyCode::Char(c) => {
            app.bottom_chat.insert_char(c);
        }
        KeyCode::Backspace => {
            app.bottom_chat.delete_char();
        }
        KeyCode::Left => {
            app.bottom_chat.move_cursor_left();
        }
        KeyCode::Right => {
            app.bottom_chat.move_cursor_right();
        }
        KeyCode::Up => {
            app.bottom_chat.history_up();
        }
        KeyCode::Down => {
            app.bottom_chat.history_down();
        }
        _ => {}
    }
}

fn handle_select_model_key(app: &mut App, key: KeyEvent) {
    let model_count = icebox_runtime::MODELS.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.mode = AppMode::Board;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.model_select_idx > 0 {
                app.model_select_idx -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.model_select_idx + 1 < model_count {
                app.model_select_idx += 1;
            }
        }
        // ←/→: adjust effort
        KeyCode::Left | KeyCode::Char('h') => {
            app.effort = app.effort.prev();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.effort = app.effort.next();
        }
        KeyCode::Enter => {
            app.apply_model_selection();
        }
        KeyCode::Char('1') => {
            app.model_select_idx = 0;
            app.apply_model_selection();
        }
        KeyCode::Char('2') if model_count > 1 => {
            app.model_select_idx = 1;
            app.apply_model_selection();
        }
        KeyCode::Char('3') if model_count > 2 => {
            app.model_select_idx = 2;
            app.apply_model_selection();
        }
        _ => {}
    }
}

fn handle_edit_key(app: &mut App, key: KeyEvent) {
    // Ctrl+S to save
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        app.save_edit_task();
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::TaskDetail;
        }
        KeyCode::Tab => {
            app.edit_field = match app.edit_field {
                EditField::Title => EditField::Body,
                EditField::Body => EditField::Title,
            };
        }
        KeyCode::Enter => match app.edit_field {
            EditField::Title => {
                // Enter in title → move to body
                app.edit_field = EditField::Body;
            }
            EditField::Body => {
                app.edit_body.push('\n');
            }
        },
        KeyCode::Char(c) => match app.edit_field {
            EditField::Title => app.edit_title.push(c),
            EditField::Body => app.edit_body.push(c),
        },
        KeyCode::Backspace => match app.edit_field {
            EditField::Title => {
                app.edit_title.pop();
            }
            EditField::Body => {
                app.edit_body.pop();
            }
        },
        _ => {}
    }
}

fn handle_create_key(app: &mut App, key: KeyEvent) {
    use crate::app::CreateField;

    const FIELD_ORDER: [CreateField; 5] = [
        CreateField::Title,
        CreateField::Tags,
        CreateField::Swimlane,
        CreateField::StartDate,
        CreateField::DueDate,
    ];

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Board;
        }
        KeyCode::Enter => {
            match app.create_field {
                CreateField::Title => app.create_field = CreateField::Tags,
                CreateField::Tags => app.create_field = CreateField::Swimlane,
                CreateField::Swimlane => app.create_field = CreateField::StartDate,
                CreateField::StartDate => app.create_field = CreateField::DueDate,
                CreateField::DueDate => {
                    app.create_task_from_input();
                    app.mode = AppMode::Board;
                }
            }
        },
        KeyCode::Char(c) => match app.create_field {
            CreateField::Title => app.create_input.push(c),
            CreateField::Tags => app.create_tags.push(c),
            CreateField::Swimlane => app.create_swimlane.push(c),
            CreateField::StartDate => app.create_start_date.push(c),
            CreateField::DueDate => app.create_due_date.push(c),
        },
        KeyCode::Backspace => match app.create_field {
            CreateField::Title => { app.create_input.pop(); }
            CreateField::Tags => { app.create_tags.pop(); }
            CreateField::Swimlane => { app.create_swimlane.pop(); }
            CreateField::StartDate => { app.create_start_date.pop(); }
            CreateField::DueDate => { app.create_due_date.pop(); }
        },
        // Tab: cycle priority
        KeyCode::Tab => {
            app.create_priority_idx = (app.create_priority_idx + 1) % PRIORITIES.len();
        }
        // Up/Down: switch field
        KeyCode::Up | KeyCode::Down => {
            let cur_idx = FIELD_ORDER
                .iter()
                .position(|f| *f == app.create_field)
                .unwrap_or(0);
            let next = if key.code == KeyCode::Down {
                (cur_idx + 1) % FIELD_ORDER.len()
            } else {
                (cur_idx + FIELD_ORDER.len() - 1) % FIELD_ORDER.len()
            };
            if let Some(field) = FIELD_ORDER.get(next) {
                app.create_field = *field;
            }
        }
        _ => {}
    }
}

const PRIORITIES: [icebox_task::model::Priority; 4] = [
    icebox_task::model::Priority::Low,
    icebox_task::model::Priority::Medium,
    icebox_task::model::Priority::High,
    icebox_task::model::Priority::Critical,
];

fn handle_confirm_delete_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            app.delete_selected_task();
            app.mode = AppMode::Board;
        }
        _ => {
            app.mode = AppMode::Board;
        }
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    let x = mouse.column;
    let y = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.drag_start_y = Some(y);
            app.drag_target = detect_drag_target(app, x, y);

            // Start text selection in any content area
            let in_selectable = matches!(
                app.drag_target,
                DragTarget::SidebarDetail | DragTarget::SidebarChat | DragTarget::BottomChat
            );
            if in_selectable {
                // Activate focus for the clicked chat area
                match app.drag_target {
                    DragTarget::BottomChat => {
                        app.bottom_chat_focused = true;
                        app.sidebar_focused = false;
                    }
                    DragTarget::SidebarChat => {
                        app.sidebar.chat_focused = true;
                        app.bottom_chat_focused = false;
                    }
                    _ => {}
                }
                app.sidebar.text_selection = Some(TextSelection {
                    start: (x, y),
                    end: (x, y),
                    active: true,
                });
                return;
            }

            app.sidebar.text_selection = None;
            handle_mouse_click(app, x, y);
        }

        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(ref mut sel) = app.sidebar.text_selection
                && sel.active
            {
                sel.end = (x, y);
                return;
            }

            let Some(start_y) = app.drag_start_y else {
                return;
            };
            let delta = i32::from(start_y) - i32::from(y);
            if delta == 0 {
                return;
            }

            match app.drag_target {
                DragTarget::SidebarDetail => {
                    if delta > 0 {
                        app.sidebar.detail_scroll =
                            app.sidebar.detail_scroll.saturating_add(delta as u16);
                    } else {
                        app.sidebar.detail_scroll = app
                            .sidebar
                            .detail_scroll
                            .saturating_sub(delta.unsigned_abs() as u16);
                    }
                }
                DragTarget::SidebarChat => {
                    if delta > 0 {
                        app.sidebar.chat_scroll =
                            app.sidebar.chat_scroll.saturating_add(delta as u16);
                    } else {
                        app.sidebar.chat_scroll = app
                            .sidebar
                            .chat_scroll
                            .saturating_sub(delta.unsigned_abs() as u16);
                    }
                }
                DragTarget::BottomChat => {
                    if delta > 0 {
                        app.bottom_chat.chat_scroll =
                            app.bottom_chat.chat_scroll.saturating_add(delta as u16);
                    } else {
                        app.bottom_chat.chat_scroll = app
                            .bottom_chat
                            .chat_scroll
                            .saturating_sub(delta.unsigned_abs() as u16);
                    }
                }
                DragTarget::None => {}
            }
            app.drag_start_y = Some(y);
        }

        MouseEventKind::Up(MouseButton::Left) => {
            if let Some(ref sel) = app.sidebar.text_selection
                && sel.active
                && sel.start != sel.end
            {
                let text = extract_selected_text(app, sel);
                if !text.is_empty() {
                    copy_to_clipboard(&text);
                    app.set_status(format!("Copied {} chars", text.len()), false);
                }
            }
            if let Some(ref mut sel) = app.sidebar.text_selection {
                sel.active = false;
            }

            app.drag_start_y = None;
            app.drag_target = DragTarget::None;
        }

        MouseEventKind::ScrollUp => {
            let target = detect_drag_target(app, x, y);
            match target {
                DragTarget::SidebarDetail => {
                    app.sidebar.detail_scroll = app.sidebar.detail_scroll.saturating_sub(3);
                }
                DragTarget::SidebarChat => {
                    app.sidebar.chat_scroll = app.sidebar.chat_scroll.saturating_sub(3);
                }
                DragTarget::BottomChat => {
                    app.bottom_chat.chat_scroll = app.bottom_chat.chat_scroll.saturating_sub(3);
                }
                DragTarget::None => {
                    let now = std::time::Instant::now();
                    let should_scroll = app
                        .last_board_scroll
                        .is_none_or(|last| now.duration_since(last).as_millis() > 80);
                    if should_scroll {
                        app.board.move_selection_up();
                        app.last_board_scroll = Some(now);
                    }
                }
            }
        }

        MouseEventKind::ScrollDown => {
            let target = detect_drag_target(app, x, y);
            match target {
                DragTarget::SidebarDetail => {
                    app.sidebar.detail_scroll = app.sidebar.detail_scroll.saturating_add(3);
                }
                DragTarget::SidebarChat => {
                    app.sidebar.chat_scroll = app.sidebar.chat_scroll.saturating_add(3);
                }
                DragTarget::BottomChat => {
                    app.bottom_chat.chat_scroll = app.bottom_chat.chat_scroll.saturating_add(3);
                }
                DragTarget::None => {
                    let now = std::time::Instant::now();
                    let should_scroll = app
                        .last_board_scroll
                        .is_none_or(|last| now.duration_since(last).as_millis() > 80);
                    if should_scroll {
                        app.board.move_selection_down();
                        app.last_board_scroll = Some(now);
                    }
                }
            }
        }

        MouseEventKind::Down(MouseButton::Right | MouseButton::Middle)
        | MouseEventKind::Up(MouseButton::Right | MouseButton::Middle)
        | MouseEventKind::Drag(MouseButton::Right | MouseButton::Middle)
        | MouseEventKind::Moved
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => {}
    }
}

fn in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn detect_drag_target(app: &App, x: u16, y: u16) -> DragTarget {
    if let Some(sidebar_rect) = app.sidebar_rect
        && in_rect(x, y, sidebar_rect)
    {
        if let Some(chat_rect) = app.sidebar.chat_rect
            && in_rect(x, y, chat_rect)
        {
            return DragTarget::SidebarChat;
        }
        return DragTarget::SidebarDetail;
    }
    if let Some(chat_rect) = app.bottom_chat_rect
        && in_rect(x, y, chat_rect)
    {
        return DragTarget::BottomChat;
    }
    DragTarget::None
}

fn handle_mouse_click(app: &mut App, x: u16, y: u16) {
    // Check if click is in swimlane bar
    if let Some(bar) = app.swimlane_bar_rect
        && y == bar.y
        && x >= bar.x
        && x < bar.x.saturating_add(bar.width)
    {
        let rel_x = x.saturating_sub(bar.x);
        // Layout: 1(pad) + 5(" All ") + for each swimlane: 1(sep) + name.len()+2
        let all_end: u16 = 6; // 1 + 5
        if rel_x < all_end {
            app.board.active_swimlane = None;
        } else {
            let mut pos = all_end;
            for (i, name) in app.board.swimlanes.iter().enumerate() {
                pos += 1; // separator
                let tab_width = name.len() as u16 + 2; // " name "
                if rel_x >= pos && rel_x < pos.saturating_add(tab_width) {
                    app.board.active_swimlane = Some(i);
                    break;
                }
                pos += tab_width;
            }
        }
        app.board.refilter();
        return;
    }

    let rects: Vec<Rect> = app.column_rects.clone();

    // Check if click is in sidebar area (task detail / input)
    if let Some(sidebar_rect) = app.sidebar_rect
        && x >= sidebar_rect.x
        && x < sidebar_rect.x.saturating_add(sidebar_rect.width)
        && y >= sidebar_rect.y
        && y < sidebar_rect.y.saturating_add(sidebar_rect.height)
    {
        let input_top = sidebar_rect.y + sidebar_rect.height.saturating_sub(4);
        if y >= input_top {
            app.sidebar_focused = true;
            app.bottom_chat_focused = false;
        }
        return;
    }

    // Check if click is in bottom chat area
    if let Some(chat_rect) = app.bottom_chat_rect
        && in_rect(x, y, chat_rect)
    {
        app.bottom_chat_focused = true;
        app.sidebar_focused = false;
        return;
    }

    for (i, col_rect) in rects.iter().enumerate() {
        let in_bounds = x >= col_rect.x
            && x < col_rect.x.saturating_add(col_rect.width)
            && y >= col_rect.y
            && y < col_rect.y.saturating_add(col_rect.height);

        if !in_bounds {
            continue;
        }

        app.board.focused_column = i;
        app.bottom_chat_focused = false;
        app.clear_status();

        let col = app.board.focused_col();
        let tasks = app.board.tasks.get(&col).map(Vec::as_slice).unwrap_or(&[]);
        if !tasks.is_empty() {
            // Match the rendering logic in column.rs exactly:
            // inner = block.inner(area) where block has Borders::ALL + Padding::horizontal(1)
            // inner.y = area.y + 1 (top border), inner.height = area.height - 2
            let inner_y = col_rect.y.saturating_add(1);
            let inner_height = col_rect.height.saturating_sub(2);
            let inner_width = col_rect.width.saturating_sub(4); // borders(2) + horizontal padding(2)

            let scroll_offset = app
                .board
                .column_states
                .get(i)
                .map_or(0, |state| state.scroll_offset);

            // Walk through rendered cards from scroll_offset, accumulating Y positions
            let mut card_y = inner_y;
            let mut matched_idx = None;
            for (task_i, task) in tasks.iter().enumerate().skip(scroll_offset) {
                if card_y >= inner_y.saturating_add(inner_height) {
                    break;
                }
                let show_sl = app.board.active_swimlane.is_none();
                let card_lines = card::card_line_count(task, inner_width, show_sl);
                let card_end = card_y.saturating_add(card_lines as u16);
                if y >= card_y && y < card_end {
                    matched_idx = Some(task_i);
                    break;
                }
                // card lines + 1 separator
                card_y = card_end.saturating_add(1);
            }

            if let Some(idx) = matched_idx {
                app.board.selected_task[i] = Some(idx);
                let task_id = tasks.get(idx).map(|t| t.id.clone());
                app.switch_sidebar_task(task_id);
                app.mode = AppMode::TaskDetail;
            }
        }
        return;
    }
}

fn handle_memory_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.active_tab = Tab::Board;
            app.mode = AppMode::Board;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.memory_entries.is_empty()
                && app.memory_selected < app.memory_entries.len().saturating_sub(1)
            {
                app.memory_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.memory_selected = app.memory_selected.saturating_sub(1);
        }
        KeyCode::Char('d') => {
            if let Some(entry) = app.memory_entries.get(app.memory_selected) {
                let id = entry.id.clone();
                if let Some(store) = &app.memory_store {
                    match store.delete(&id) {
                        Ok(true) => {
                            app.reload_memory();
                            if app.memory_selected > 0
                                && app.memory_selected >= app.memory_entries.len()
                            {
                                app.memory_selected = app.memory_entries.len().saturating_sub(1);
                            }
                            app.set_status("Memory deleted", false);
                        }
                        Ok(false) => app.set_status("Memory not found", true),
                        Err(e) => app.set_status(format!("Delete failed: {e}"), true),
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            app.reload_memory();
            app.set_status("Memory refreshed", false);
        }
        KeyCode::Char('/') => {
            app.bottom_chat_open = !app.bottom_chat_open;
            app.bottom_chat_focused = app.bottom_chat_open;
        }
        _ => {}
    }
}

fn normalize_selection(sel: &TextSelection) -> ((u16, u16), (u16, u16)) {
    if sel.start.1 < sel.end.1 || (sel.start.1 == sel.end.1 && sel.start.0 <= sel.end.0) {
        (sel.start, sel.end)
    } else {
        (sel.end, sel.start)
    }
}

fn extract_selected_text(app: &App, sel: &TextSelection) -> String {
    let (start, _end) = normalize_selection(sel);

    // Determine which area the selection started in
    let (area_rect, lines, scroll) = if let Some(chat_rect) = app.sidebar.chat_rect
        && in_rect(start.0, start.1, chat_rect)
    {
        (
            chat_rect,
            &app.sidebar.rendered_chat_lines,
            app.sidebar.chat_scroll,
        )
    } else if let Some(chat_rect) = app.bottom_chat_rect
        && in_rect(start.0, start.1, chat_rect)
    {
        (
            chat_rect,
            &app.bottom_chat.rendered_chat_lines,
            app.bottom_chat.chat_scroll,
        )
    } else if let Some(detail_rect) = app.sidebar.detail_rect {
        (
            detail_rect,
            &app.sidebar.rendered_text_lines,
            app.sidebar.detail_scroll,
        )
    } else {
        return String::new();
    };

    let (start, end) = normalize_selection(sel);
    let mut result = String::new();

    for y in start.1..=end.1 {
        if y < area_rect.y || y >= area_rect.y.saturating_add(area_rect.height) {
            continue;
        }
        let line_idx = usize::from(y.saturating_sub(area_rect.y).saturating_add(scroll));
        let line = lines.get(line_idx).map(String::as_str).unwrap_or("");

        let x_start = if y == start.1 {
            usize::from(start.0.saturating_sub(area_rect.x))
        } else {
            0
        };
        let x_end = if y == end.1 {
            usize::from(end.0.saturating_sub(area_rect.x))
        } else {
            usize::MAX
        };

        let chars: Vec<char> = line.chars().collect();
        let mut col = 0usize;
        let mut char_start = 0usize;
        let mut char_end = chars.len();
        let mut found_start = false;

        for (i, &ch) in chars.iter().enumerate() {
            if !found_start && col >= x_start {
                char_start = i;
                found_start = true;
            }
            col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
            if col >= x_end {
                char_end = i + 1;
                break;
            }
        }
        if !found_start {
            char_start = chars.len();
        }

        let selected: String = chars
            .get(char_start..char_end.min(chars.len()))
            .unwrap_or(&[])
            .iter()
            .collect();
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(selected.trim_end());
    }

    result
}

fn copy_to_clipboard(text: &str) {
    if cfg!(target_os = "macos") {
        let child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(ref mut stdin) = child.stdin {
                let _ = std::io::Write::write_all(stdin, text.as_bytes());
            }
            let _ = child.wait();
        }
        return;
    }
    let child = std::process::Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn();
    if let Ok(mut child) = child {
        if let Some(ref mut stdin) = child.stdin {
            let _ = std::io::Write::write_all(stdin, text.as_bytes());
        }
        let _ = child.wait();
    }
}
