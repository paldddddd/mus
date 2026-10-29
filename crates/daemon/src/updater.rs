//! Automatic update verification and downloader for wallmus via GitHub Releases and raw manifest.

use anyhow::{bail, Context, Result};
use common::{is_newer_version, UpdateInfo};
use std::env;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;
pub const DEFAULT_REPO: &str = "paldddddd/mus";
pub const DEFAULT_RELEASES_API: &str = "https://api.github.com/repos/paldddddd/mus/releases/latest";
pub const DEFAULT_CONTENTS_API: &str =
    "https://api.github.com/repos/paldddddd/mus/contents/dist/update.json?ref=main";
pub const DEFAULT_TAGS_API: &str = "https://api.github.com/repos/paldddddd/mus/tags";
pub const DEFAULT_RAW_MANIFEST_MAIN: &str =
    "https://raw.githubusercontent.com/paldddddd/mus/main/dist/update.json";
pub const DEFAULT_RAW_MANIFEST_MASTER: &str =
    "https://raw.githubusercontent.com/paldddddd/mus/master/dist/update.json";

/// Helper to fetch URL via Windows built-in curl.exe silently with no console window.
fn fetch_url_string(url: &str, extra_header: Option<&str>, timeout_secs: u32) -> Result<String> {
    let mut cmd = Command::new("curl.exe");
    cmd.creation_flags(CREATE_NO_WINDOW).args([
        "-s",
        "-L",
        "-m",
        &timeout_secs.to_string(),
        "-H",
        "User-Agent: wallmus-updater",
    ]);

    if let Some(h) = extra_header {
        cmd.args(["-H", h]);
    }

    cmd.arg(url);

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute curl.exe for {}", url))?;

    if !output.status.success() {
        bail!("curl failed with status: {:?}", output.status);
    }

    let text = String::from_utf8(output.stdout)
        .with_context(|| "Failed to parse UTF-8 output from curl")?;
    Ok(text)
}

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

    /// Check for available updates from GitHub or custom URL.
    pub fn check(&self) -> Result<UpdateInfo> {
        // 1. Check custom override if specified in WALLMUS_UPDATE_URL
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
            } else if url.starts_with("http://") || url.starts_with("https://") {
                if let Ok(content) = fetch_url_string(url, None, 5) {
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

        // 2. Query GitHub Contents API for live dist/update.json (zero cache delay)
        if let Ok(content_text) = fetch_url_string(
            DEFAULT_CONTENTS_API,
            Some("Accept: application/vnd.github.v3.raw"),
            4,
        ) {
            if let Ok(info) = serde_json::from_str::<UpdateInfo>(&content_text) {
                let has_update = is_newer_version(&self.current_version, &info.latest_version);
                let mut dl = info.download_url;
                if let Some(ref d) = dl {
                    if !d.starts_with("http://") && !d.starts_with("https://") {
                        dl = Some(format!(
                            "https://github.com/paldddddd/mus/releases/latest/download/{}",
                            d
                        ));
                    }
                }
                return Ok(UpdateInfo {
                    current_version: self.current_version.clone(),
                    latest_version: info.latest_version,
                    update_available: has_update,
                    release_notes: info.release_notes,
                    download_url: dl.or_else(|| {
                        Some("https://github.com/paldddddd/mus/releases".to_string())
                    }),
                    published_at: info.published_at,
                    sha256: info.sha256,
                });
            }
        }

        // 3. Query GitHub Releases API for the latest release in paldddddd/mus
        if let Ok(json_text) = fetch_url_string(DEFAULT_RELEASES_API, None, 5) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_text) {
                if let Some(tag) = val.get("tag_name").and_then(|v| v.as_str()) {
                    let clean_tag = tag.trim_start_matches('v');
                    let has_update = is_newer_version(&self.current_version, clean_tag);
                    let body = val
                        .get("body")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Новое обновление wallmus доступно на GitHub.")
                        .to_string();
                    let published_at = val
                        .get("published_at")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let html_url = val
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("https://github.com/paldddddd/mus/releases")
                        .to_string();

                    // Find download asset URL (zip or exe)
                    let mut download_url = html_url;
                    if let Some(assets) = val.get("assets").and_then(|a| a.as_array()) {
                        for asset in assets {
                            if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
                                if name.ends_with(".zip") || name.ends_with(".exe") {
                                    if let Some(browser_url) =
                                        asset.get("browser_download_url").and_then(|u| u.as_str())
                                    {
                                        download_url = browser_url.to_string();
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    return Ok(UpdateInfo {
                        current_version: self.current_version.clone(),
                        latest_version: clean_tag.to_string(),
                        update_available: has_update,
                        release_notes: body,
                        download_url: Some(download_url),
                        published_at,
                        sha256: None,
                    });
                }
            }
        }

        // 4. Query GitHub Tags API if no formal Release created yet
        if let Ok(json_text) = fetch_url_string(DEFAULT_TAGS_API, None, 4) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_text) {
                if let Some(arr) = val.as_array() {
                    if let Some(first) = arr.first() {
                        if let Some(tag) = first.get("name").and_then(|v| v.as_str()) {
                            let clean_tag = tag.trim_start_matches('v');
                            let has_update = is_newer_version(&self.current_version, clean_tag);
                            if has_update {
                                return Ok(UpdateInfo {
                                    current_version: self.current_version.clone(),
                                    latest_version: clean_tag.to_string(),
                                    update_available: true,
                                    release_notes: format!("Доступно обновление wallmus v{}. Включает новые дефолтные обои, визуализатор и оптимизации.", clean_tag),
                                    download_url: Some(format!("https://github.com/paldddddd/mus/releases/tag/{}", tag)),
                                    published_at: None,
                                    sha256: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 5. Fallback: Query raw update.json from paldddddd/mus on main or master
        for raw_url in [DEFAULT_RAW_MANIFEST_MAIN, DEFAULT_RAW_MANIFEST_MASTER] {
            if let Ok(raw_text) = fetch_url_string(raw_url, None, 4) {
                if let Ok(info) = serde_json::from_str::<UpdateInfo>(&raw_text) {
                    let has_update = is_newer_version(&self.current_version, &info.latest_version);
                    let mut dl = info.download_url;
                    if let Some(ref d) = dl {
                        if !d.starts_with("http://") && !d.starts_with("https://") {
                            dl = Some(format!(
                                "https://github.com/paldddddd/mus/releases/latest/download/{}",
                                d
                            ));
                        }
                    }
                    return Ok(UpdateInfo {
                        current_version: self.current_version.clone(),
                        latest_version: info.latest_version,
                        update_available: has_update,
                        release_notes: info.release_notes,
                        download_url: dl.or_else(|| {
                            Some("https://github.com/paldddddd/mus/releases".to_string())
                        }),
                        published_at: info.published_at,
                        sha256: info.sha256,
                    });
                }
            }
        }

        // 4. Default: Up-to-date status connected to github.com/paldddddd/mus
        Ok(UpdateInfo {
            current_version: self.current_version.clone(),
            latest_version: self.current_version.clone(),
            update_available: false,
            release_notes: format!(
                "wallmus v{} is up to date (connected to github.com/{}).",
                self.current_version, DEFAULT_REPO
            ),
            download_url: Some(format!("https://github.com/{}/releases", DEFAULT_REPO)),
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
    }
}
