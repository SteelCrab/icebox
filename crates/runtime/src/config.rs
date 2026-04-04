//! Icebox configuration persistence.
//! Stores user preferences in `~/.icebox/config.json` (or `$XDG_CONFIG_HOME/icebox/config.json` on Linux).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// User preferences persisted across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IceboxConfig {
    /// Last-used model ID (e.g., "claude-opus-4-6").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl IceboxConfig {
    /// Config file path, respecting `ICEBOX_CONFIG_HOME` and `XDG_CONFIG_HOME`.
    fn config_path() -> PathBuf {
        if let Ok(path) = std::env::var("ICEBOX_CONFIG_HOME")
            && !path.is_empty()
        {
            return PathBuf::from(path).join("config.json");
        }

        #[cfg(target_os = "linux")]
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
            && !xdg.is_empty()
        {
            return PathBuf::from(xdg).join("icebox").join("config.json");
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".icebox").join("config.json")
    }

    /// Load config from disk. Returns default on any error.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(&path, &json).with_context(|| format!("failed to write {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    /// Save the last-used model to config.
    pub fn save_model(model: &str) -> Result<()> {
        let mut config = Self::load();
        config.model = Some(model.to_string());
        config.save()
    }

    /// Get the saved model, if any.
    pub fn saved_model() -> Option<String> {
        Self::load().model
    }
}
