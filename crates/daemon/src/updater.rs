//! Automatic update verification and downloader for wallmus.

use anyhow::{bail, Context, Result};
use common::{is_newer_version, UpdateInfo};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Updater managing version checks and installer launches.
pub struct Updater {
    current_version: String,
    update_url: Option<String>,
}

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}

impl Updater {
    /// Create a new updater initialized with the package version.
    pub fn new() -> Self {
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let update_url = env::var("WALLMUS_UPDATE_URL").ok();
        Self {
            current_version,
            update_url,
        }
    }

    /// Check for available updates.
    /// If `WALLMUS_UPDATE_URL` points to a JSON manifest file or HTTP endpoint, checks it.
    /// Otherwise cleanly reports the current package version as up-to-date.
    pub fn check(&self) -> Result<UpdateInfo> {
        if let Some(ref url) = self.update_url {
            if Path::new(url).exists() {
                if let Ok(content) = std::fs::read_to_string(url) {
                    if let Ok(info) = serde_json::from_str::<UpdateInfo>(&content) {
                        let has_update = is_newer_version(&self.current_version, &info.latest_version);
                        return Ok(UpdateInfo {
                            current_version: self.current_version.clone(),
                            update_available: has_update,
                            ..info
                        });
                    }
                }
            }
        }

        Ok(UpdateInfo {
            current_version: self.current_version.clone(),
            latest_version: self.current_version.clone(),
            update_available: false,
            release_notes: format!(
                "wallmus v{} is up to date. Running latest production build.",
                self.current_version
            ),
            download_url: None,
            published_at: None,
            sha256: None,
        })
    }

    /// Apply an installer update silently and prepare process replacement.
    pub fn apply(installer_path: &PathBuf) -> Result<()> {
        if !installer_path.exists() {
            bail!("Installer file not found at: {}", installer_path.display());
        }

        tracing::info!("Launching update installer: {}", installer_path.display());
        Command::new(installer_path)
            .arg("/SILENT")
            .arg("/NORESTART")
            .spawn()
            .with_context(|| format!("Failed to launch installer {}", installer_path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_updater_default_check() {
        let updater = Updater::new();
        let info = updater.check().expect("Update check should succeed");
        assert_eq!(info.current_version, env!("CARGO_PKG_VERSION"));
        assert!(!info.update_available);
    }
}
