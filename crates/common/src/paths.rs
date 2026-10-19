use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Application name used for folders and IPC named pipes.
pub const APP_NAME: &str = "wallmus";

/// Check if the application is running in portable mode.
///
/// Portable mode is active if a marker file `portable.dat` exists next to the current
/// executable or in the current working directory. All configs, databases, wallpapers,
/// and logs are then isolated strictly inside a local `data/` directory.
pub fn is_portable_mode() -> bool {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            if parent.join("portable.dat").exists() {
                return true;
            }
        }
    }
    std::path::Path::new("portable.dat").exists()
}

/// Retrieve `%APPDATA%/wallmus/` or `<exe_dir>/data/` if running in portable mode.
pub fn app_data_dir() -> PathBuf {
    if is_portable_mode() {
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                return parent.join("data");
            }
        }
        return PathBuf::from(".").join("data");
    }

    if let Ok(appdata) = env::var("APPDATA") {
        PathBuf::from(appdata).join(APP_NAME)
    } else {
        PathBuf::from(".").join(".wallmus_data")
    }
}

/// Retrieve `%LOCALAPPDATA%/wallmus/logs/` or `<exe_dir>/data/logs/` if in portable mode.
pub fn logs_dir() -> PathBuf {
    if is_portable_mode() {
        return app_data_dir().join("logs");
    }

    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        PathBuf::from(local_appdata).join(APP_NAME).join("logs")
    } else {
        app_data_dir().join("logs")
    }
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("config.toml")
}

pub fn db_path() -> PathBuf {
    app_data_dir().join("library.db")
}

pub fn wallpapers_dir() -> PathBuf {
    app_data_dir().join("wallpapers")
}

pub fn thumbs_dir() -> PathBuf {
    app_data_dir().join("thumbs")
}

pub fn covers_dir() -> PathBuf {
    app_data_dir().join("covers")
}

/// Ensure all standard application directories exist.
pub fn ensure_app_dirs() -> Result<()> {
    fs::create_dir_all(app_data_dir()).context("Failed to create APPDATA directory")?;
    fs::create_dir_all(wallpapers_dir()).context("Failed to create wallpapers directory")?;
    fs::create_dir_all(thumbs_dir()).context("Failed to create thumbs directory")?;
    fs::create_dir_all(covers_dir()).context("Failed to create covers directory")?;
    fs::create_dir_all(logs_dir()).context("Failed to create logs directory")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_default_and_structure() {
        let app_dir = app_data_dir();
        assert!(app_dir.to_string_lossy().len() > 0);
        let cfg = config_path();
        assert!(cfg.ends_with("config.toml"));
        let db = db_path();
        assert!(db.ends_with("library.db"));
    }

    #[test]
    fn test_portable_mode_logic() {
        // Without portable.dat in test environment
        let is_p = std::path::Path::new("portable.dat").exists();
        assert_eq!(is_portable_mode(), is_p);
    }
}
