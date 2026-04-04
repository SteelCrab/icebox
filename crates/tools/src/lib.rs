use anyhow::{Context, Result};
use icebox_api::ToolDefinition;
use icebox_task::model::{Column, Priority, Task};
use icebox_task::store::TaskStore;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct IceboxToolExecutor {
    workspace: PathBuf,
    store: Arc<Mutex<TaskStore>>,
    memory_store: icebox_task::memory::MemoryStore,
}

impl IceboxToolExecutor {
    /// # Errors
    /// Returns an error if the memory store directory cannot be created.
    pub fn new(workspace: PathBuf, store: Arc<Mutex<TaskStore>>) -> Result<Self> {
        let memory_store = icebox_task::memory::MemoryStore::open(&workspace)?;
        Ok(Self {
            workspace,
            store,
            memory_store,
        })
    }
}

impl icebox_runtime::ToolExecutor for IceboxToolExecutor {
    fn execute(&self, tool_name: &str, input: &str) -> Result<String> {
        // Strip common prefixes that models sometimes hallucinate
        let tool_name = tool_name
            .strip_prefix("mcp_")
            .or_else(|| tool_name.strip_prefix("icebox_"))
            .unwrap_or(tool_name);
        match tool_name {
            "bash" => execute_bash(input),
            "read_file" => execute_read_file(input, &self.workspace),
            "write_file" => execute_write_file(input, &self.workspace),
            "glob_search" => execute_glob_search(input, &self.workspace),
            "grep_search" => execute_grep_search(input, &self.workspace),
            "list_tasks" => execute_list_tasks(&self.store),
            "create_task" => execute_create_task(input, &self.store),
            "update_task" => execute_update_task(input, &self.store),
            "move_task" => execute_move_task(input, &self.store),
            "save_memory" => execute_save_memory(input, &self.memory_store),
            "list_memories" => execute_list_memories(&self.memory_store),
            "delete_memory" => execute_delete_memory(input, &self.memory_store),
            _ => Ok(format!("Unknown tool: {tool_name}")),
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "bash".to_string(),
                description: Some("Icebox built-in tool. Execute a shell command and return stdout/stderr.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The shell command to execute" }
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "read_file".to_string(),
                description: Some("Icebox built-in tool. Read file contents from the workspace.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path (relative to workspace or absolute)" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write_file".to_string(),
                description: Some("Icebox built-in tool. Write or overwrite a file in the workspace.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path (relative to workspace or absolute)" },
                        "content": { "type": "string", "description": "Content to write" }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "glob_search".to_string(),
                description: Some("Icebox built-in tool. Find files matching a glob pattern in the workspace.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob pattern (e.g. **/*.rs, src/**/*.ts)" }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "grep_search".to_string(),
                description: Some("Icebox built-in tool. Search file contents for a regex pattern.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Regex pattern to search for" },
                        "path": { "type": "string", "description": "Directory or file to search in (default: workspace root)" }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolDefinition {
                name: "list_tasks".to_string(),
                description: Some(
                    "Icebox built-in tool. List all kanban board tasks grouped by column (Icebox/Emergency/InProgress/Testing/Complete).".to_string(),
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "create_task".to_string(),
                description: Some("Icebox built-in tool. Create a new task on the kanban board.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Task title" },
                        "column": { "type": "string", "description": "Target column", "enum": ["icebox", "emergency", "inprogress", "testing", "complete"] },
                        "priority": { "type": "string", "description": "Task priority", "enum": ["low", "medium", "high", "critical"] },
                        "start_date": { "type": "string", "description": "Start date (ISO8601, e.g. 2026-04-03T00:00:00Z)" },
                        "due_date": { "type": "string", "description": "Due date (ISO8601, e.g. 2026-04-10T00:00:00Z)" }
                    },
                    "required": ["title"]
                }),
            },
            ToolDefinition {
                name: "update_task".to_string(),
                description: Some("Icebox built-in tool. Update fields of an existing task (title, tags, priority, dates, body).".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "Task ID (or prefix) to update" },
                        "title": { "type": "string", "description": "New title" },
                        "priority": { "type": "string", "description": "New priority", "enum": ["low", "medium", "high", "critical"] },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "New tags (replaces existing)" },
                        "start_date": { "type": "string", "description": "Start date (ISO8601, e.g. 2026-04-03T00:00:00Z). Use empty string to clear." },
                        "due_date": { "type": "string", "description": "Due date (ISO8601, e.g. 2026-04-10T00:00:00Z). Use empty string to clear." },
                        "body": { "type": "string", "description": "New body text (markdown)" }
                    },
                    "required": ["task_id"]
                }),
            },
            ToolDefinition {
                name: "move_task".to_string(),
                description: Some("Icebox built-in tool. Move a task to a different kanban column.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "Task ID (or prefix) to move" },
                        "column": { "type": "string", "description": "Target column", "enum": ["icebox", "emergency", "inprogress", "testing", "complete"] }
                    },
                    "required": ["task_id", "column"]
                }),
            },
            ToolDefinition {
                name: "save_memory".to_string(),
                description: Some("Icebox built-in tool. Save important information to persistent memory for future AI context across sessions.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "The information to remember" },
                        "source": { "type": "string", "description": "Source context (task ID or 'global')" }
                    },
                    "required": ["content"]
                }),
            },
            ToolDefinition {
                name: "list_memories".to_string(),
                description: Some("Icebox built-in tool. List all saved persistent memories.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "delete_memory".to_string(),
                description: Some("Icebox built-in tool. Delete a saved memory by ID.".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "memory_id": { "type": "string", "description": "Memory ID to delete" }
                    },
                    "required": ["memory_id"]
                }),
            },
        ]
    }
}

// --- Tool implementations ---

#[derive(Deserialize)]
struct BashInput {
    command: String,
}

fn execute_bash(input: &str) -> Result<String> {
    let parsed: BashInput = serde_json::from_str(input).context("invalid bash input")?;
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&parsed.command)
        .output()
        .context("failed to execute command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("STDERR:\n");
        result.push_str(&stderr);
    }
    if result.is_empty() {
        result.push_str("(no output)");
    }
    Ok(result)
}

#[derive(Deserialize)]
struct ReadFileInput {
    path: String,
}

fn execute_read_file(input: &str, workspace: &Path) -> Result<String> {
    let parsed: ReadFileInput = serde_json::from_str(input).context("invalid read_file input")?;
    let path = resolve_path(workspace, &parsed.path);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read: {}", path.display()))?;
    Ok(content)
}

#[derive(Deserialize)]
struct WriteFileInput {
    path: String,
    content: String,
}

fn execute_write_file(input: &str, workspace: &Path) -> Result<String> {
    let parsed: WriteFileInput = serde_json::from_str(input).context("invalid write_file input")?;
    let path = resolve_path(workspace, &parsed.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create dir: {}", parent.display()))?;
    }
    std::fs::write(&path, &parsed.content)
        .with_context(|| format!("failed to write: {}", path.display()))?;
    Ok(format!(
        "Written {} bytes to {}",
        parsed.content.len(),
        path.display()
    ))
}

#[derive(Deserialize)]
struct GlobInput {
    pattern: String,
}

fn execute_glob_search(input: &str, workspace: &Path) -> Result<String> {
    let parsed: GlobInput = serde_json::from_str(input).context("invalid glob_search input")?;
    let full_pattern = workspace.join(&parsed.pattern);
    let pattern_str = full_pattern.to_string_lossy();
    let mut results = Vec::new();
    for entry in glob::glob(&pattern_str).context("invalid glob pattern")? {
        match entry {
            Ok(path) => {
                let relative = path.strip_prefix(workspace).unwrap_or(&path);
                results.push(relative.display().to_string());
            }
            Err(e) => results.push(format!("error: {e}")),
        }
    }
    if results.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

#[derive(Deserialize)]
struct GrepInput {
    pattern: String,
    path: Option<String>,
}

fn execute_grep_search(input: &str, workspace: &Path) -> Result<String> {
    let parsed: GrepInput = serde_json::from_str(input).context("invalid grep_search input")?;
    let search_path = match &parsed.path {
        Some(p) => resolve_path(workspace, p),
        None => workspace.to_path_buf(),
    };
    let re = regex::Regex::new(&parsed.pattern).context("invalid regex pattern")?;
    let mut results = Vec::new();

    walk_and_grep(&search_path, workspace, &re, &mut results)?;

    if results.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

fn walk_and_grep(
    path: &Path,
    workspace: &Path,
    re: &regex::Regex,
    results: &mut Vec<String>,
) -> Result<()> {
    if results.len() >= 100 {
        return Ok(());
    }

    for entry in walkdir::WalkDir::new(path)
        .max_depth(10)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if results.len() >= 100 {
            break;
        }
        let file_path = entry.path();
        if !file_path.is_file() {
            continue;
        }
        // Skip binary/large files
        let metadata = std::fs::metadata(file_path).ok();
        if metadata.as_ref().is_some_and(|m| m.len() > 1_000_000) {
            continue;
        }

        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let relative = file_path.strip_prefix(workspace).unwrap_or(file_path);
        for (line_num, line) in content.lines().enumerate() {
            if re.is_match(line) {
                results.push(format!(
                    "{}:{}: {}",
                    relative.display(),
                    line_num + 1,
                    line.trim()
                ));
                if results.len() >= 100 {
                    break;
                }
            }
        }
    }
    Ok(())
}

// --- Kanban-specific tools ---

fn execute_list_tasks(store: &Arc<Mutex<TaskStore>>) -> Result<String> {
    let store = store
        .lock()
        .map_err(|e| anyhow::anyhow!("lock error: {e}"))?;
    let tasks = store.tasks_by_column()?;
    let mut output = String::new();
    for col in Column::ALL {
        output.push_str(&format!("\n## {}\n", col.display_name()));
        match tasks.get(&col) {
            Some(col_tasks) if !col_tasks.is_empty() => {
                for task in col_tasks {
                    output.push_str(&format!(
                        "- [{}] {} ({}) {}\n",
                        task.id.chars().take(8).collect::<String>(),
                        task.title,
                        task.priority.label(),
                        if task.tags.is_empty() {
                            String::new()
                        } else {
                            format!("[{}]", task.tags.join(", "))
                        }
                    ));
                }
            }
            _ => {
                output.push_str("  (empty)\n");
            }
        }
    }
    Ok(output)
}

#[derive(Deserialize)]
struct CreateTaskInput {
    title: String,
    column: Option<String>,
    priority: Option<String>,
    start_date: Option<String>,
    due_date: Option<String>,
}

fn execute_create_task(input: &str, store: &Arc<Mutex<TaskStore>>) -> Result<String> {
    let parsed: CreateTaskInput =
        serde_json::from_str(input).context("invalid create_task input")?;

    let column = match parsed.column.as_deref() {
        Some("emergency") => Column::Emergency,
        Some("inprogress") => Column::InProgress,
        Some("testing") => Column::Testing,
        Some("complete") => Column::Complete,
        _ => Column::Icebox,
    };

    let priority = match parsed.priority.as_deref() {
        Some("low") => Priority::Low,
        Some("high") => Priority::High,
        Some("critical") => Priority::Critical,
        _ => Priority::Medium,
    };

    let mut task = Task::new(parsed.title.clone(), column, priority);
    if let Some(s) = &parsed.start_date {
        task.start_date = chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok();
    }
    if let Some(s) = &parsed.due_date {
        task.due_date = chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok();
    }
    let id = task.id.clone();
    let store = store
        .lock()
        .map_err(|e| anyhow::anyhow!("lock error: {e}"))?;
    store.save(&task)?;

    Ok(format!(
        "Created task '{}' (id: {id}) in {}",
        parsed.title,
        column.display_name()
    ))
}

#[derive(Deserialize)]
struct UpdateTaskInput {
    task_id: String,
    title: Option<String>,
    priority: Option<String>,
    tags: Option<Vec<String>>,
    start_date: Option<String>,
    due_date: Option<String>,
    body: Option<String>,
}

fn execute_update_task(input: &str, store: &Arc<Mutex<TaskStore>>) -> Result<String> {
    let parsed: UpdateTaskInput =
        serde_json::from_str(input).context("invalid update_task input")?;

    let store = store
        .lock()
        .map_err(|e| anyhow::anyhow!("lock error: {e}"))?;

    let task_id = find_task_by_prefix(&store, &parsed.task_id)?;
    let mut task = store.get(&task_id)?;
    let mut changes = Vec::new();

    if let Some(title) = &parsed.title {
        task.title = title.clone();
        changes.push("title");
    }
    if let Some(p) = &parsed.priority {
        task.priority = match p.as_str() {
            "low" => Priority::Low,
            "high" => Priority::High,
            "critical" => Priority::Critical,
            _ => Priority::Medium,
        };
        changes.push("priority");
    }
    if let Some(tags) = &parsed.tags {
        task.tags = tags.clone();
        changes.push("tags");
    }
    if let Some(s) = &parsed.start_date {
        if s.is_empty() {
            task.start_date = None;
        } else {
            task.start_date = chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok();
        }
        changes.push("start_date");
    }
    if let Some(s) = &parsed.due_date {
        if s.is_empty() {
            task.due_date = None;
        } else {
            task.due_date = chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok();
        }
        changes.push("due_date");
    }
    if let Some(body) = &parsed.body {
        task.body = body.clone();
        changes.push("body");
    }

    task.updated_at = chrono::Utc::now();
    store.save(&task)?;

    Ok(format!(
        "Updated task '{}' (id: {}): {}",
        task.title,
        task.id,
        changes.join(", ")
    ))
}

#[derive(Deserialize)]
struct MoveTaskInput {
    task_id: String,
    column: String,
}

fn find_task_by_prefix(store: &TaskStore, prefix: &str) -> Result<String> {
    let tasks = store.list()?;
    let matching: Vec<&Task> = tasks.iter().filter(|t| t.id.starts_with(prefix)).collect();
    match matching.len() {
        0 => anyhow::bail!("No task found with ID starting with '{prefix}'"),
        1 => Ok(matching[0].id.clone()),
        n => anyhow::bail!("Ambiguous: {n} tasks match ID prefix '{prefix}'"),
    }
}

fn execute_move_task(input: &str, store: &Arc<Mutex<TaskStore>>) -> Result<String> {
    let parsed: MoveTaskInput = serde_json::from_str(input).context("invalid move_task input")?;

    let column = match parsed.column.as_str() {
        "icebox" => Column::Icebox,
        "emergency" => Column::Emergency,
        "inprogress" => Column::InProgress,
        "testing" => Column::Testing,
        "complete" => Column::Complete,
        other => return Ok(format!("Unknown column: {other}")),
    };

    let store = store
        .lock()
        .map_err(|e| anyhow::anyhow!("lock error: {e}"))?;

    let task_id = find_task_by_prefix(&store, &parsed.task_id)?;
    let mut task = store.get(&task_id)?;
    task.column = column;
    task.updated_at = chrono::Utc::now();
    store.save(&task)?;
    Ok(format!(
        "Moved '{}' to {}",
        task.title,
        column.display_name()
    ))
}

// --- Memory tools ---

#[derive(Deserialize)]
struct SaveMemoryInput {
    content: String,
    source: Option<String>,
}

fn execute_save_memory(
    input: &str,
    store: &icebox_task::memory::MemoryStore,
) -> Result<String> {
    let parsed: SaveMemoryInput =
        serde_json::from_str(input).context("invalid save_memory input")?;
    let source = parsed.source.unwrap_or_else(|| "global".into());
    let entry = store.add(parsed.content, source)?;
    Ok(format!("Memory saved (id: {})", entry.id))
}

fn execute_list_memories(store: &icebox_task::memory::MemoryStore) -> Result<String> {
    let entries = store.list()?;
    if entries.is_empty() {
        return Ok("No memories saved.".to_string());
    }
    let mut output = String::from("Memories:\n");
    for entry in &entries {
        let date = entry.created_at.format("%Y-%m-%d %H:%M");
        let short_id: String = entry.id.chars().take(8).collect();
        output.push_str(&format!(
            "- [{short_id}] ({date}) [{src}] {content}\n",
            src = entry.source,
            content = entry.content
        ));
    }
    Ok(output)
}

#[derive(Deserialize)]
struct DeleteMemoryInput {
    memory_id: String,
}

fn execute_delete_memory(
    input: &str,
    store: &icebox_task::memory::MemoryStore,
) -> Result<String> {
    let parsed: DeleteMemoryInput =
        serde_json::from_str(input).context("invalid delete_memory input")?;
    if store.delete(&parsed.memory_id)? {
        Ok(format!("Memory {} deleted.", parsed.memory_id))
    } else {
        Ok(format!("Memory {} not found.", parsed.memory_id))
    }
}

fn resolve_path(workspace: &Path, relative: &str) -> PathBuf {
    let path = Path::new(relative);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    }
}
