use crate::column::{ColumnState, ColumnWidget};
use crate::layout::AppLayout;
use icebox_task::model::{Column, Task};
use ratatui::buffer::Buffer;
use ratatui::widgets::StatefulWidget;
use std::collections::{BTreeMap, BTreeSet};

pub struct BoardState {
    pub focused_column: usize,
    pub selected_task: [Option<usize>; 5],
    pub column_states: [ColumnState; 5],
    pub all_tasks: BTreeMap<Column, Vec<Task>>,
    pub tasks: BTreeMap<Column, Vec<Task>>,
    pub swimlanes: Vec<String>,
    pub active_swimlane: Option<usize>,
}

impl BoardState {
    pub fn new(tasks: BTreeMap<Column, Vec<Task>>) -> Self {
        let swimlanes = Self::collect_swimlanes(&tasks);
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
            all_tasks: tasks.clone(),
            tasks,
            swimlanes,
            active_swimlane: None,
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
        self.swimlanes = Self::collect_swimlanes(&tasks);
        if let Some(idx) = self.active_swimlane
            && idx >= self.swimlanes.len()
        {
            self.active_swimlane = None;
        }
        self.all_tasks = tasks;
        self.tasks =
            Self::apply_swimlane_filter(&self.all_tasks, &self.swimlanes, self.active_swimlane);
        self.clamp_selections();
    }

    pub fn next_swimlane(&mut self) {
        let total = self.swimlanes.len();
        self.active_swimlane = match self.active_swimlane {
            None if total > 0 => Some(0),
            Some(idx) if idx + 1 < total => Some(idx + 1),
            _ => None,
        };
        self.refilter();
    }

    pub fn prev_swimlane(&mut self) {
        let total = self.swimlanes.len();
        self.active_swimlane = match self.active_swimlane {
            None if total > 0 => Some(total - 1),
            Some(0) => None,
            Some(idx) => Some(idx - 1),
            None => None,
        };
        self.refilter();
    }

    pub fn active_swimlane_name(&self) -> Option<&str> {
        self.active_swimlane
            .and_then(|idx| self.swimlanes.get(idx))
            .map(String::as_str)
    }

    pub fn refilter(&mut self) {
        self.tasks =
            Self::apply_swimlane_filter(&self.all_tasks, &self.swimlanes, self.active_swimlane);
        self.clamp_selections();
    }

    fn clamp_selections(&mut self) {
        for (i, col) in Column::ALL.iter().enumerate() {
            let count = match self.tasks.get(col) {
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
    }

    fn collect_swimlanes(tasks: &BTreeMap<Column, Vec<Task>>) -> Vec<String> {
        let mut set = BTreeSet::new();
        for col_tasks in tasks.values() {
            for task in col_tasks {
                if let Some(ref name) = task.swimlane {
                    set.insert(name.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    fn apply_swimlane_filter(
        all_tasks: &BTreeMap<Column, Vec<Task>>,
        swimlanes: &[String],
        active_swimlane: Option<usize>,
    ) -> BTreeMap<Column, Vec<Task>> {
        let filter_name = active_swimlane.and_then(|idx| swimlanes.get(idx));
        match filter_name {
            None => all_tasks.clone(),
            Some(name) => {
                let mut filtered = BTreeMap::new();
                for (col, col_tasks) in all_tasks {
                    let matching: Vec<Task> = col_tasks
                        .iter()
                        .filter(|t| t.swimlane.as_deref() == Some(name.as_str()))
                        .cloned()
                        .collect();
                    filtered.insert(*col, matching);
                }
                filtered
            }
        }
    }

    pub fn render(&mut self, layout: &AppLayout, buf: &mut Buffer) {
        let show_swimlane = self.active_swimlane.is_none();
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
                show_swimlane,
            };
            let Some(state) = self.column_states.get_mut(i) else {
                continue;
            };
            widget.render(area, buf, state);
        }
    }
}
