use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Application name used for folders and IPC named pipes.
pub const APP_NAME: &str = "wallmus";

/// Retrieve `%APPDATA%/wallmus/` directory path.
pub fn app_data_dir() -> PathBuf {
    if let Ok(appdata) = env::var("APPDATA") {
        PathBuf::from(appdata).join(APP_NAME)
    } else {
        PathBuf::from(".").join(".wallmus_data")
    }
}

/// Retrieve `%LOCALAPPDATA%/wallmus/logs/` directory path.
pub fn logs_dir() -> PathBuf {
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
