use anyhow::{Context, Result};

const REPO_OWNER: &str = "SteelCrab";
const REPO_NAME: &str = "icebox";
const BIN_NAME: &str = "icebox";

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
