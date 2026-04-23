pub mod settings;

use std::fs;
use std::path::{Path, PathBuf};
use directories::ProjectDirs;
use crate::config::settings::Settings;

pub fn get_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn get_config_path() -> Option<PathBuf> {
    get_config_dir().map(|dir| dir.join("config.toml"))
}

/// Parse TOML content into `Settings`. On parse failure — malformed syntax,
/// truncated file, wrong type, missing required section — log a warning and
/// fall back to `Settings::default()`. Never panics.
///
/// WAT-103 (R-14 mitigation): a corrupted or hand-edited `config.toml` must
/// not brick startup. Earlier versions used `toml::from_str(..).unwrap_or_default()`
/// which swallowed the error silently; users had no signal that their edit
/// didn't take. This function surfaces the error on stderr until WAT-406
/// (notifications drawer) can render it in-app.
pub fn parse_settings(content: &str) -> Settings {
    match toml::from_str::<Settings>(content) {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!(
                "watson: failed to parse config.toml ({e}); falling back to default settings"
            );
            Settings::default()
        }
    }
}

/// Load settings from an explicit path. Returns `Settings::default()` if the
/// file is missing or unreadable. Used by tests and by `load_settings()`.
pub fn load_settings_from(path: &Path) -> Settings {
    if !path.exists() {
        return Settings::default();
    }
    match fs::read_to_string(path) {
        Ok(content) => parse_settings(&content),
        Err(e) => {
            eprintln!(
                "watson: failed to read config at {}: {e}; falling back to default settings",
                path.display()
            );
            Settings::default()
        }
    }
}

pub fn load_settings() -> Settings {
    let path = match get_config_path() {
        Some(p) => p,
        None => return Settings::default(),
    };

    if !path.exists() {
        // First run: persist defaults so the user has something to edit.
        let settings = Settings::default();
        let _ = save_settings(&settings);
        return settings;
    }

    load_settings_from(&path)
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let dir = get_config_dir().ok_or("Could not determine config directory")?;
    let path = dir.join("config.toml");

    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let content = toml::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests;
