use crate::model::Task;
use anyhow::{Context, Result};

const FRONTMATTER_DELIMITER: &str = "---";

pub fn parse_task(content: &str) -> Result<Task> {
    let (yaml_str, body) =
        split_frontmatter(content).context("invalid frontmatter: missing --- delimiters")?;

    let mut task: Task =
        serde_yaml::from_str(yaml_str).context("failed to parse YAML frontmatter")?;
    task.body = body.to_string();
    Ok(task)
}

pub fn serialize_task(task: &Task) -> Result<String> {
    let yaml = serde_yaml::to_string(task).context("failed to serialize task metadata")?;
    let mut out = String::with_capacity(yaml.len() + task.body.len() + 16);
    out.push_str(FRONTMATTER_DELIMITER);
    out.push('\n');
    out.push_str(&yaml);
    out.push_str(FRONTMATTER_DELIMITER);
    out.push('\n');
    if !task.body.is_empty() {
        out.push('\n');
        out.push_str(&task.body);
        if !task.body.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let trimmed = content.trim_start();
    let rest = trimmed.strip_prefix(FRONTMATTER_DELIMITER)?;
    let rest = match rest.strip_prefix('\n') {
        Some(stripped) => stripped,
        None => rest,
    };

    let end = rest.find(&format!("\n{FRONTMATTER_DELIMITER}"))?;
    let yaml = &rest[..end];
    let after = &rest[end + 1 + FRONTMATTER_DELIMITER.len()..];
    let body = match after.strip_prefix('\n') {
        Some(stripped) => stripped,
        None => after,
    };
    let body = match body.strip_prefix('\n') {
        Some(stripped) => stripped,
        None => body,
    };

    Some((yaml, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Column, Priority};

    #[test]
    fn roundtrip() -> Result<()> {
        let mut task = Task::new("Test task".into(), Column::Icebox, Priority::Medium);
        task.body = "## Description\nSome content here.\n".into();

        let serialized = serialize_task(&task)?;
        let parsed = parse_task(&serialized)?;

        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.title, task.title);
        assert_eq!(parsed.column, task.column);
        assert_eq!(parsed.priority, task.priority);
        assert!(parsed.body.contains("Some content here."));
        Ok(())
    }

    #[test]
    fn missing_frontmatter_returns_error() {
        let content = "No frontmatter here";
        let result = parse_task(content);
        assert!(result.is_err());
    }

    #[test]
    fn empty_body() -> Result<()> {
        let mut task = Task::new("Empty body".into(), Column::Emergency, Priority::High);
        task.body = String::new();

        let serialized = serialize_task(&task)?;
        let parsed = parse_task(&serialized)?;

        assert_eq!(parsed.id, task.id);
        assert!(parsed.body.is_empty() || parsed.body.trim().is_empty());
        Ok(())
    }
}
