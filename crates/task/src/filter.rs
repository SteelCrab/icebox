use crate::model::{Column, Priority, Task};

pub fn filter_by_column(tasks: &[Task], column: Column) -> Vec<&Task> {
    tasks.iter().filter(|t| t.column == column).collect()
}

pub fn filter_by_priority(tasks: &[Task], priority: Priority) -> Vec<&Task> {
    tasks.iter().filter(|t| t.priority == priority).collect()
}

pub fn search_by_title<'a>(tasks: &'a [Task], query: &str) -> Vec<&'a Task> {
    let query_lower = query.to_lowercase();
    tasks
        .iter()
        .filter(|t| t.title.to_lowercase().contains(&query_lower))
        .collect()
}
