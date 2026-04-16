use anyhow::{Context, Result};
use std::path::PathBuf;

const REPO_OWNER: &str = "SteelCrab";
const REPO_NAME: &str = "icebox";
const BIN_NAME: &str = "icebox";

// ── Version check (background, cached 24h) ──

#[derive(serde::Serialize, serde::Deserialize)]
struct VersionCache {
    latest_version: String,
    checked_at: u64,
}

fn config_dir() -> Option<PathBuf> {
    std::env::var("ICEBOX_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .ok()
                .filter(|s| !s.is_empty())
                .map(|h| PathBuf::from(h).join(".icebox"))
        })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let stripped = v.strip_prefix('v').unwrap_or(v);
    let mut parts = stripped.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // Strip any pre-release/build suffix from the patch component
    let patch_raw = parts.next()?;
    let patch_end = patch_raw
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(patch_raw.len());
    let patch = patch_raw.get(..patch_end)?.parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    matches!(
        (parse_semver(latest), parse_semver(current)),
        (Some(l), Some(c)) if l > c
    )
}

fn fetch_latest_version() -> Option<String> {
    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .ok()?
        .fetch()
        .ok()?;
    releases.first().map(|r| r.version.clone())
}

/// Check GitHub for a newer release. Returns `Some(version)` when an update
/// is available, `None` when up-to-date or on error.
/// Results are cached for 24 hours in `~/.icebox/version_check.json`.
pub fn check_for_update() -> Option<String> {
    let current = env!("CARGO_PKG_VERSION");
    let cache_path = config_dir()?.join("version_check.json");

    const DAY: u64 = 24 * 60 * 60;

    // Try cached result first
    if let Ok(data) = std::fs::read_to_string(&cache_path)
        && let Ok(cache) = serde_json::from_str::<VersionCache>(&data)
        && now_secs().saturating_sub(cache.checked_at) < DAY
    {
        return if is_newer(&cache.latest_version, current) {
            Some(cache.latest_version)
        } else {
            None
        };
    }

    // Cache stale or missing — fetch from GitHub
    let latest = fetch_latest_version()?;

    // Update cache (ignore write errors)
    if let Ok(json) = serde_json::to_string(&VersionCache {
        latest_version: latest.clone(),
        checked_at: now_secs(),
    }) {
        let _ = std::fs::write(&cache_path, json);
    }

    if is_newer(&latest, current) {
        Some(latest)
    } else {
        None
    }
}

// ── Self-update (icebox upgrade) ──

pub fn run() -> Result<()> {
    println!("Icebox Self-Update");
    println!("──────────────────");

    let current = env!("CARGO_PKG_VERSION");
    println!("  Current version: v{current}");
    println!("  Source: github.com/{REPO_OWNER}/{REPO_NAME}");
    println!("  Checking GitHub for latest release…");
    println!();

    let updater = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(true)
        .current_version(current)
        .build()
        .context("failed to configure self-update")?;

    let status = updater
        .update()
        .context("failed to apply update — try downloading manually from GitHub releases")?;

    println!();
    if status.updated() {
        println!("  ✓ Updated to v{}", status.version());
        println!("  Run `icebox` to start using the new version.");
    } else {
        println!("  Already at latest version (v{current})");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn newer_version_detected() {
        assert!(is_newer("0.5.1", "0.5.0"));
        assert!(is_newer("0.6.0", "0.5.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn same_version_is_not_newer() {
        assert!(!is_newer("0.5.0", "0.5.0"));
    }

    #[test]
    fn older_version_is_not_newer() {
        assert!(!is_newer("0.4.9", "0.5.0"));
        assert!(!is_newer("0.5.0", "0.5.1"));
    }

    #[test]
    fn v_prefix_is_ignored() {
        assert!(is_newer("v0.5.1", "0.5.0"));
        assert!(is_newer("v0.5.1", "v0.5.0"));
        assert!(!is_newer("v0.5.0", "v0.5.0"));
    }

    #[test]
    fn prerelease_suffix_in_patch_is_stripped() {
        // 0.5.1-beta is treated as 0.5.1, which is newer than 0.5.0
        assert!(is_newer("0.5.1-beta", "0.5.0"));
    }

    #[test]
    fn invalid_version_returns_false() {
        assert!(!is_newer("garbage", "0.5.0"));
        assert!(!is_newer("0.5.0", "garbage"));
    }
}
