use anyhow::{Context, Result};
use std::path::PathBuf;

const REPO_OWNER: &str = "SteelCrab";
const REPO_NAME: &str = "icebox";
const BIN_NAME: &str = "icebox";

/// Return a GitHub token from `GITHUB_TOKEN` or `GH_TOKEN` when set, so that
/// `self_update`'s API calls are authenticated (5000 req/hour instead of 60).
fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok())
        .filter(|s| !s.is_empty())
}

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
    let mut builder = self_update::backends::github::ReleaseList::configure();
    builder.repo_owner(REPO_OWNER).repo_name(REPO_NAME);
    if let Some(token) = github_token() {
        builder.auth_token(&token);
    }
    let releases = builder.build().ok()?.fetch().ok()?;
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

// ── Interactive upgrade prompt (called on `icebox` / `icebox web` startup) ──

/// If a newer release is available and we're attached to a TTY, ask the user
/// whether to upgrade now. On `y`, run `upgrade::run()` and exit so the new
/// binary is used on the next launch. On `n`, network failure, fresh cache,
/// or non-interactive stdio, return silently.
pub fn prompt_and_upgrade_if_available() -> Result<()> {
    use std::io::{IsTerminal, Write, stdin, stdout};

    let Some(latest) = check_for_update() else {
        return Ok(());
    };

    // Skip in non-interactive contexts (piped stdin, MCP stdio, CI, daemon).
    if !stdin().is_terminal() || !stdout().is_terminal() {
        return Ok(());
    }

    let current = env!("CARGO_PKG_VERSION");
    println!();
    println!("  Update available: v{current} -> v{latest}");
    print!("  Upgrade now? [Y/n] ");
    let _ = stdout().flush();

    let mut input = String::new();
    if stdin().read_line(&mut input).is_err() {
        return Ok(());
    }
    let yes = matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    );
    if !yes {
        return Ok(());
    }

    match run() {
        Ok(()) => {
            println!();
            println!("  Restart icebox to use v{latest}.");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!();
            eprintln!("  Upgrade failed: {e:#}");
            eprintln!("  Continuing with v{current}.");
            Ok(())
        }
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

    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(true)
        .current_version(current)
        // Skip self_update's own [Y/n] prompt — we already asked the user
        // via prompt_and_upgrade_if_available(), and on Windows PowerShell
        // the inner prompt can lose stdin echo and appear unresponsive.
        .no_confirm(true);
    if let Some(token) = github_token() {
        builder.auth_token(&token);
    }
    let updater = builder.build().context("failed to configure self-update")?;

    let status = updater.update().map_err(|e| {
        let msg = e.to_string();
        if msg.contains("403") {
            anyhow::anyhow!(
                "GitHub API returned 403 — likely rate-limited (60 req/hour without auth). \
                 Set GITHUB_TOKEN (or GH_TOKEN) to raise the limit to 5000 req/hour, \
                 or wait a few minutes and retry.\n  Underlying error: {msg}"
            )
        } else {
            anyhow::anyhow!(
                "failed to apply update — try downloading manually from GitHub releases: {msg}"
            )
        }
    })?;

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
