pub mod settings;

use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;
use crate::config::settings::Settings;

pub fn get_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn get_config_path() -> Option<PathBuf> {
    get_config_dir().map(|dir| dir.join("config.toml"))
}

pub fn load_settings() -> Settings {
    let path = match get_config_path() {
        Some(p) => p,
        None => return Settings::default(),
    };

    if !path.exists() {
        let settings = Settings::default();
        let _ = save_settings(&settings);
        return settings;
    }

    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let dir = get_config_dir().ok_or("Could not determine config directory")?;
    let path = dir.join("config.toml");

    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let content = toml::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;

    Ok(())
}
