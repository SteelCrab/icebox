use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

pub struct MemoryStore {
    memory_path: PathBuf,
}

impl MemoryStore {
    pub fn open(workspace: &Path) -> Result<Self> {
        let memory_path = workspace.join(".icebox").join("memory.json");
        if let Some(parent) = memory_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        Ok(Self { memory_path })
    }

    pub fn list(&self) -> Result<Vec<MemoryEntry>> {
        if !self.memory_path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.memory_path)
            .with_context(|| format!("failed to read {}", self.memory_path.display()))?;
        let entries: Vec<MemoryEntry> =
            serde_json::from_str(&content).context("failed to parse memory.json")?;
        Ok(entries)
    }

    pub fn add(&self, content: String, source: String) -> Result<MemoryEntry> {
        let mut entries = self.list()?;
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            content,
            source,
            created_at: Utc::now(),
        };
        entries.push(entry.clone());
        self.save(&entries)?;
        Ok(entry)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut entries = self.list()?;
        let len_before = entries.len();
        entries.retain(|e| e.id != id);
        if entries.len() == len_before {
            return Ok(false);
        }
        self.save(&entries)?;
        Ok(true)
    }

    pub fn search(&self, query: &str) -> Result<Vec<MemoryEntry>> {
        let query_lower = query.to_lowercase();
        let entries = self.list()?;
        Ok(entries
            .into_iter()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .collect())
    }

    fn save(&self, entries: &[MemoryEntry]) -> Result<()> {
        let json =
            serde_json::to_string_pretty(entries).context("failed to serialize memory")?;
        let tmp_path = self.memory_path.with_extension("json.tmp");
        fs::write(&tmp_path, &json)
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &self.memory_path)
            .with_context(|| format!("failed to rename to {}", self.memory_path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn add_and_list() -> Result<()> {
        let dir = TempDir::new()?;
        let store = MemoryStore::open(dir.path())?;

        assert!(store.list()?.is_empty());

        store.add("test memory".into(), "global".into())?;
        let entries = store.list()?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "test memory");
        assert_eq!(entries[0].source, "global");
        Ok(())
    }

    #[test]
    fn delete_entry() -> Result<()> {
        let dir = TempDir::new()?;
        let store = MemoryStore::open(dir.path())?;

        let entry = store.add("to delete".into(), "global".into())?;
        assert!(store.delete(&entry.id)?);
        assert!(store.list()?.is_empty());
        assert!(!store.delete("nonexistent")?);
        Ok(())
    }

    #[test]
    fn search_entries() -> Result<()> {
        let dir = TempDir::new()?;
        let store = MemoryStore::open(dir.path())?;

        store.add("PostgreSQL 16 사용".into(), "global".into())?;
        store.add("Redis cache layer".into(), "task-1".into())?;

        let results = store.search("postgres")?;
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("PostgreSQL"));

        let results = store.search("layer")?;
        assert_eq!(results.len(), 1);
        Ok(())
    }
}
