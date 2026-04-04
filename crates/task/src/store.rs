use crate::frontmatter;
use crate::model::{Column, Task};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Initialize the `.icebox/` workspace structure at the given path.
/// Creates `tasks/`, `sessions/`, and `.gitignore`.
/// Returns `true` if freshly created, `false` if already existed.
pub fn init_workspace(workspace: &Path) -> Result<bool> {
    let icebox_dir = workspace.join(".icebox");
    let already_exists = icebox_dir.exists();

    let tasks_dir = icebox_dir.join("tasks");
    let sessions_dir = icebox_dir.join("sessions");
    fs::create_dir_all(&tasks_dir)
        .with_context(|| format!("failed to create {}", tasks_dir.display()))?;
    fs::create_dir_all(&sessions_dir)
        .with_context(|| format!("failed to create {}", sessions_dir.display()))?;

    let gitignore_path = icebox_dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, "sessions/\nmemory.json\n")
            .with_context(|| format!("failed to write {}", gitignore_path.display()))?;
    }

    Ok(!already_exists)
}

pub struct TaskStore {
    tasks_dir: PathBuf,
}

impl TaskStore {
    pub fn open(workspace: &Path) -> Result<Self> {
        init_workspace(workspace)?;
        let tasks_dir = workspace.join(".icebox").join("tasks");
        Ok(Self { tasks_dir })
    }

    pub fn list(&self) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        if !self.tasks_dir.exists() {
            return Ok(tasks);
        }
        for entry in fs::read_dir(&self.tasks_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                match self.load_file(&path) {
                    Ok(task) => tasks.push(task),
                    Err(e) => eprintln!("warning: skipping {}: {e}", path.display()),
                }
            }
        }
        tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.created_at.cmp(&b.created_at))
        });
        Ok(tasks)
    }

    pub fn get(&self, id: &str) -> Result<Task> {
        let path = self.task_path(id);
        self.load_file(&path)
            .with_context(|| format!("task not found: {id}"))
    }

    pub fn save(&self, task: &Task) -> Result<()> {
        let content = frontmatter::serialize_task(task)?;
        let path = self.task_path(&task.id);
        let tmp_path = path.with_extension("md.tmp");
        fs::write(&tmp_path, &content)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("failed to rename to {}", path.display()))?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.task_path(id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to delete {}", path.display()))?;
        }
        Ok(())
    }

    pub fn tasks_by_column(&self) -> Result<BTreeMap<Column, Vec<Task>>> {
        let mut map: BTreeMap<Column, Vec<Task>> = BTreeMap::new();
        for col in Column::ALL {
            map.insert(col, Vec::new());
        }
        for task in self.list()? {
            map.entry(task.column).or_default().push(task);
        }
        Ok(map)
    }

    fn task_path(&self, id: &str) -> PathBuf {
        self.tasks_dir.join(format!("{id}.md"))
    }

    fn load_file(&self, path: &Path) -> Result<Task> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        frontmatter::parse_task(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_workspace_creates_structure() -> Result<()> {
        let dir = tempfile::TempDir::new()?;
        let fresh = init_workspace(dir.path())?;
        assert!(fresh);
        assert!(dir.path().join(".icebox/tasks").is_dir());
        assert!(dir.path().join(".icebox/sessions").is_dir());
        assert!(dir.path().join(".icebox/.gitignore").is_file());

        let gitignore = fs::read_to_string(dir.path().join(".icebox/.gitignore"))?;
        assert!(gitignore.contains("sessions/"));
        assert!(gitignore.contains("memory.json"));
        Ok(())
    }

    #[test]
    fn init_workspace_idempotent() -> Result<()> {
        let dir = tempfile::TempDir::new()?;
        let fresh = init_workspace(dir.path())?;
        assert!(fresh);
        let fresh2 = init_workspace(dir.path())?;
        assert!(!fresh2);
        Ok(())
    }
}
