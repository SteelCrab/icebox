use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    Icebox,
    Emergency,
    #[serde(alias = "inprogress")]
    InProgress,
    Testing,
    Complete,
}

impl Column {
    pub const ALL: [Column; 5] = [
        Column::Icebox,
        Column::Emergency,
        Column::InProgress,
        Column::Testing,
        Column::Complete,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            Column::Icebox => "ICEBOX",
            Column::Emergency => "EMERGENCY",
            Column::InProgress => "IN PROGRESS",
            Column::Testing => "TESTING",
            Column::Complete => "COMPLETE",
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Column::Icebox => "icebox",
            Column::Emergency => "emergency",
            Column::InProgress => "inprogress",
            Column::Testing => "testing",
            Column::Complete => "complete",
        }
    }

    pub fn next(&self) -> Option<Column> {
        match self {
            Column::Icebox => Some(Column::Emergency),
            Column::Emergency => Some(Column::InProgress),
            Column::InProgress => Some(Column::Testing),
            Column::Testing => Some(Column::Complete),
            Column::Complete => None,
        }
    }

    pub fn prev(&self) -> Option<Column> {
        match self {
            Column::Icebox => None,
            Column::Emergency => Some(Column::Icebox),
            Column::InProgress => Some(Column::Emergency),
            Column::Testing => Some(Column::InProgress),
            Column::Complete => Some(Column::Testing),
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Column::Icebox => 0,
            Column::Emergency => 1,
            Column::InProgress => 2,
            Column::Testing => 3,
            Column::Complete => 4,
        }
    }

    pub fn from_index(idx: usize) -> Option<Column> {
        Column::ALL.get(idx).copied()
    }
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

impl Priority {
    pub fn symbol(&self) -> &'static str {
        match self {
            Priority::Low => "○",
            Priority::Medium => "◑",
            Priority::High => "●",
            Priority::Critical => "◉",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Priority::Low => "Low",
            Priority::Medium => "Medium",
            Priority::High => "High",
            Priority::Critical => "Critical",
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub column: Column,
    pub priority: Priority,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swimlane: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub body: String,
}

/// Progress tracking (e.g., 3/10 subtasks done)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

impl Progress {
    /// Returns a display string like "3/10"
    #[must_use]
    pub fn display(&self) -> String {
        format!("{}/{}", self.done, self.total)
    }

    /// Returns completion percentage (0-100)
    #[must_use]
    pub fn percent(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        (self.done * 100) / self.total
    }
}

impl Task {
    pub fn new(title: String, column: Column, priority: Priority) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            column,
            priority,
            tags: Vec::new(),
            depends_on: Vec::new(),
            start_date: None,
            due_date: None,
            progress: None,
            swimlane: None,
            created_at: now,
            updated_at: now,
            body: String::new(),
        }
    }
}
