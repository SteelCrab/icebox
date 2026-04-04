use crate::column::{ColumnState, ColumnWidget};
use crate::layout::AppLayout;
use icebox_task::model::{Column, Task};
use ratatui::buffer::Buffer;
use ratatui::widgets::StatefulWidget;
use std::collections::BTreeMap;

pub struct BoardState {
    pub focused_column: usize,
    pub selected_task: [Option<usize>; 5],
    pub column_states: [ColumnState; 5],
    pub tasks: BTreeMap<Column, Vec<Task>>,
}

impl BoardState {
    pub fn new(tasks: BTreeMap<Column, Vec<Task>>) -> Self {
        let mut selected_task = [None; 5];
        for (i, col) in Column::ALL.iter().enumerate() {
            if let Some(col_tasks) = tasks.get(col)
                && !col_tasks.is_empty()
            {
                selected_task[i] = Some(0);
            }
        }
        Self {
            focused_column: 0,
            selected_task,
            column_states: Default::default(),
            tasks,
        }
    }

    pub fn focused_col(&self) -> Column {
        match Column::from_index(self.focused_column) {
            Some(col) => col,
            None => Column::Icebox,
        }
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let col = self.focused_col();
        let idx = self.selected_task[self.focused_column]?;
        let col_tasks = self.tasks.get(&col)?;
        col_tasks.get(idx)
    }

    pub fn column_task_count(&self, col: Column) -> usize {
        match self.tasks.get(&col) {
            Some(tasks) => tasks.len(),
            None => 0,
        }
    }

    pub fn move_focus_left(&mut self) {
        if self.focused_column > 0 {
            self.focused_column -= 1;
        }
    }

    pub fn move_focus_right(&mut self) {
        if self.focused_column < 4 {
            self.focused_column += 1;
        }
    }

    pub fn move_selection_up(&mut self) {
        let col = self.focused_column;
        if let Some(ref mut sel) = self.selected_task[col] {
            *sel = sel.saturating_sub(1);
        }
    }

    pub fn move_selection_down(&mut self) {
        let col = self.focused_column;
        let count = self.column_task_count(self.focused_col());
        match self.selected_task[col] {
            Some(ref mut sel) if *sel + 1 < count => {
                *sel += 1;
            }
            None if count > 0 => {
                self.selected_task[col] = Some(0);
            }
            _ => {}
        }
    }

    pub fn reload(&mut self, tasks: BTreeMap<Column, Vec<Task>>) {
        for (i, col) in Column::ALL.iter().enumerate() {
            let count = match tasks.get(col) {
                Some(col_tasks) => col_tasks.len(),
                None => 0,
            };
            match self.selected_task[i] {
                Some(_) if count == 0 => {
                    self.selected_task[i] = None;
                }
                Some(ref mut sel) if *sel >= count => {
                    *sel = count.saturating_sub(1);
                }
                None if count > 0 => {
                    self.selected_task[i] = Some(0);
                }
                _ => {}
            }
        }
        self.tasks = tasks;
    }

    pub fn render(&mut self, layout: &AppLayout, buf: &mut Buffer) {
        for (i, col) in Column::ALL.iter().enumerate() {
            let Some(area) = layout.columns.get(i).copied() else {
                continue;
            };
            let tasks_slice = match self.tasks.get(col) {
                Some(tasks) => tasks.as_slice(),
                None => &[],
            };
            let widget = ColumnWidget {
                column: *col,
                tasks: tasks_slice,
                focused: i == self.focused_column,
                selected_task: self.selected_task[i],
            };
            let Some(state) = self.column_states.get_mut(i) else {
                continue;
            };
            widget.render(area, buf, state);
        }
    }
}
