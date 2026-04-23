pub mod migrations;
pub mod settings;

use std::fs;
use std::path::{Path, PathBuf};
use directories::ProjectDirs;
use crate::config::settings::Settings;

/// WAT-206: highest settings schema version this binary knows how to load.
/// A config file with a higher version is rejected and the app falls back
/// to defaults with a startup warning — we don't know what fields mean in
/// the future, so pretending to load them would silently drop data.
/// Bump this when adding a migration in `config/migrations.rs`.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// WAT-205 / R-10: query prefixes that shadow user-defined web-search
/// keywords if reused.
///
/// The list is a superset of `search::dispatch::RESERVED_PREFIXES`
/// (router-level reserved prefixes — notes / files / clipboard) plus
/// the UI-level prefixes that the SearchBar or downstream dispatchers
/// interpret specially: `s` (scratchpad chained shortcut), `>` (system
/// command prefix). A web-search keyword equal to any of these is
/// effectively unreachable, so the settings UI warns the user.
///
/// Returns a `Vec` rather than a `&[&str]` so it can be serialized
/// directly by the Tauri command bridge.
pub fn user_reserved_prefixes() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::search::dispatch::RESERVED_PREFIXES.to_vec();
    // UI-level reserved. Kept in sync manually with:
    // - `SearchBar.tsx` empty-query keyboard shortcuts (s, `, n, N, f)
    // - `lib.rs::search` command-prefix detection (>).
    // `n`, `f`, and `cb` are already in the router list above.
    // The backtick `` ` `` is a keyboard char only, not a query prefix,
    // so it's omitted here.
    out.extend(["s", ">"]);
    out
}

/// Non-fatal conditions the caller of `load_*_with_warnings` should surface
/// to the UI. Intentionally free of Tauri / frontend types so `config` can
/// stay a leaf module; the caller translates into `warnings::StartupWarning`.
#[derive(Debug, Clone)]
pub enum SettingsWarning {
    /// The on-disk config was written by a Watson newer than this binary.
    /// We're running with defaults; the user's real config is untouched on
    /// disk and they'll get it back when they upgrade again.
    FromNewerVersion {
        file_version: u32,
        supported_version: u32,
    },
}

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
///
/// WAT-206: also enforces the schema version. A file newer than
/// `CURRENT_SCHEMA_VERSION` is rejected (defaults + `FromNewerVersion`
/// warning); an older file is migrated forward via `migrations::migrate`.
pub fn parse_settings(content: &str) -> (Settings, Option<SettingsWarning>) {
    let mut settings = match toml::from_str::<Settings>(content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "watson: failed to parse config.toml ({e}); falling back to default settings"
            );
            return (Settings::default(), None);
        }
    };

    // WAT-206: newer-version gate. Defensive — we can't read fields we
    // don't know about, and we don't want to silently overwrite them on
    // the next save. Fall back to defaults in memory; leave disk intact.
    if settings.schema_version > CURRENT_SCHEMA_VERSION {
        eprintln!(
            "watson: config.toml was written by a newer Watson (schema v{}); this binary \
             supports up to v{}. Running with defaults; your config is preserved on disk.",
            settings.schema_version, CURRENT_SCHEMA_VERSION
        );
        return (
            Settings::default(),
            Some(SettingsWarning::FromNewerVersion {
                file_version: settings.schema_version,
                supported_version: CURRENT_SCHEMA_VERSION,
            }),
        );
    }

    // Older or equal: migrate forward (no-op when already current).
    migrations::migrate(&mut settings);
    (settings, None)
}

/// Load settings from an explicit path. Returns `Settings::default()` if the
/// file is missing or unreadable. Used by tests and by `load_settings()`.
pub fn load_settings_from(path: &Path) -> (Settings, Option<SettingsWarning>) {
    if !path.exists() {
        return (Settings::default(), None);
    }
    match fs::read_to_string(path) {
        Ok(content) => parse_settings(&content),
        Err(e) => {
            eprintln!(
                "watson: failed to read config at {}: {e}; falling back to default settings",
                path.display()
            );
            (Settings::default(), None)
        }
    }
}

pub fn load_settings() -> (Settings, Option<SettingsWarning>) {
    let path = match get_config_path() {
        Some(p) => p,
        None => return (Settings::default(), None),
    };

    if !path.exists() {
        // First run: persist defaults so the user has something to edit.
        let settings = Settings::default();
        let _ = save_settings(&settings);
        return (settings, None);
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
