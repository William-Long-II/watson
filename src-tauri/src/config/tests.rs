#[cfg(test)]
mod tests {
    use crate::config::{load_settings_from, parse_settings};
    use crate::config::settings::Settings;
    use std::collections::HashSet;
    use tempfile::TempDir;

    /// Prefixes reserved by the search dispatcher in `lib.rs::search`.
    /// A web-search keyword colliding with one of these silently shadows
    /// the reserved feature (notes / files / clipboard / scratchpad /
    /// system commands) and is effectively unreachable.
    const RESERVED_PREFIXES: &[&str] =
        &["n", "notes", "f", "files", "cb", "clip", "s", ">"];

    #[test]
    fn default_settings_match_documented_contract() {
        let settings = Settings::default();
        assert_eq!(settings.activation.hotkey, "Alt+Space");
        assert_eq!(settings.search.max_results, 8);
        assert!(settings.general.launch_at_login);
    }

    #[test]
    fn default_web_searches_include_all_required_keywords() {
        let settings = Settings::default();
        let keywords: HashSet<&str> = settings
            .web_searches
            .iter()
            .map(|w| w.keyword.as_str())
            .collect();

        for required in ["g", "ddg", "yt", "gh", "wiki", "so", "jira"] {
            assert!(
                keywords.contains(required),
                "default web searches missing keyword '{required}' (have: {keywords:?})"
            );
        }
    }

    #[test]
    fn default_web_search_keywords_are_unique() {
        let settings = Settings::default();
        let keywords: Vec<&str> = settings
            .web_searches
            .iter()
            .map(|w| w.keyword.as_str())
            .collect();
        let unique: HashSet<&str> = keywords.iter().copied().collect();
        assert_eq!(
            keywords.len(),
            unique.len(),
            "web search keywords must be unique (have: {keywords:?})"
        );
    }

    #[test]
    fn default_web_search_keywords_do_not_collide_with_reserved_prefixes() {
        let settings = Settings::default();
        for ws in &settings.web_searches {
            assert!(
                !RESERVED_PREFIXES.contains(&ws.keyword.as_str()),
                "web search '{}' uses reserved prefix '{}' — would be shadowed by search dispatcher",
                ws.name,
                ws.keyword,
            );
        }
    }

    // --- WAT-103: malformed TOML fallback ---
    //
    // Every branch in these tests ends with a valid `Settings` value. The
    // regression we're guarding against is a prior behavior where
    // `unwrap_or_default` swallowed the parse error silently, and a bad edit
    // could also cause startup panics in debug builds. The contract now:
    // malformed input → defaults, no panic, error logged to stderr.

    #[test]
    fn parse_settings_returns_defaults_on_empty_input() {
        // An empty file fails to satisfy any required struct field; must not
        // panic. The `activation.hotkey` assertion proves we landed on
        // `Settings::default()` rather than some partially-populated state.
        let settings = parse_settings("");
        assert_eq!(settings.activation.hotkey, "Alt+Space");
    }

    #[test]
    fn parse_settings_returns_defaults_on_truncated_input() {
        // File was cut mid-array — classic hand-edit accident.
        let broken = r#"
[general]
launch_at_login = true

[search]
max_results = 8
web_searches = [
  { name = "Google", keyword = "g", url = "https://www.google.com/search?q={query}"
"#;
        let settings = parse_settings(broken);
        // Full fallback: every default value is present, nothing from the
        // partial input leaked through.
        assert_eq!(settings.search.max_results, 8);
        assert!(settings.general.launch_at_login);
        assert!(
            !settings.web_searches.is_empty(),
            "default web searches should be present when parse fails"
        );
    }

    #[test]
    fn parse_settings_returns_defaults_on_missing_required_section() {
        // `[general]`, `[activation]`, `[search]`, `[theme]` are required
        // top-level sections. Missing any one of them → whole parse fails →
        // full defaults. (Field-level fallback would require custom
        // Deserialize impls — a separate ticket.)
        let partial = r#"
[activation]
hotkey = "Alt+Space"
show_tray_icon = true
"#;
        let settings = parse_settings(partial);
        // If parse had succeeded, hotkey from the file would survive and
        // launch_at_login would come from serde default. Since it fails, we
        // get full defaults — both values come from Settings::default().
        assert!(settings.general.launch_at_login);
    }

    #[test]
    fn parse_settings_returns_defaults_on_wrong_type() {
        // Hand-edited "max_results = 'eight'" — number field quoted as string.
        // TOML parse succeeds syntactically; serde fails on the type. Must
        // fall back, not crash.
        let bad_type = r#"
[general]
launch_at_login = true

[activation]
hotkey = "Alt+Space"
show_tray_icon = true

[search]
max_results = "eight"
show_recently_used = true
fuzzy_match_threshold = 0.6

[theme]
mode = "system"
accent_color = "system"
"#;
        let settings = parse_settings(bad_type);
        assert_eq!(settings.search.max_results, 8);
    }

    #[test]
    fn parse_settings_ignores_unknown_top_level_keys() {
        // Forward-compat: a config written by a newer Watson with fields we
        // don't know about must still parse. serde's default for structs is
        // "ignore unknown fields" (we do NOT set deny_unknown_fields); pin
        // that behavior so no one accidentally tightens it.
        let with_extra = r#"
future_feature = "enabled"

[general]
launch_at_login = false

[activation]
hotkey = "Alt+Space"
show_tray_icon = true

[search]
max_results = 12
show_recently_used = true
fuzzy_match_threshold = 0.6

[theme]
mode = "dark"
accent_color = "blue"
"#;
        let settings = parse_settings(with_extra);
        // Real values from the file survive; no fallback triggered.
        assert_eq!(settings.search.max_results, 12);
        assert_eq!(settings.theme.mode, "dark");
        assert!(!settings.general.launch_at_login);
    }

    #[test]
    fn parse_settings_succeeds_on_a_full_valid_config() {
        // Negative-control: defaults serialized + parsed round-trips cleanly.
        // If this test ever fails, parse_settings itself is broken, not the
        // fallback path.
        let defaults = Settings::default();
        let serialized = toml::to_string_pretty(&defaults).expect("serialize defaults");
        let parsed = parse_settings(&serialized);
        assert_eq!(parsed.search.max_results, defaults.search.max_results);
        assert_eq!(parsed.activation.hotkey, defaults.activation.hotkey);
        assert_eq!(parsed.web_searches.len(), defaults.web_searches.len());
    }

    #[test]
    fn load_settings_from_returns_defaults_for_missing_file() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("never-created.toml");
        let settings = load_settings_from(&missing);
        assert_eq!(settings.activation.hotkey, "Alt+Space");
    }

    #[test]
    fn load_settings_from_returns_defaults_for_malformed_file() {
        // End-to-end: a real file with bad contents produces defaults, not
        // a panic.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml = [").unwrap();
        let settings = load_settings_from(&path);
        assert_eq!(settings.activation.hotkey, "Alt+Space");
    }

    #[test]
    fn load_settings_from_round_trips_a_saved_config() {
        // Pin the serialize → write → read → parse chain. If the TOML
        // emitter ever drifts from the deserializer, this catches it.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut settings = Settings::default();
        settings.search.max_results = 42;
        let content = toml::to_string_pretty(&settings).unwrap();
        std::fs::write(&path, content).unwrap();

        let loaded = load_settings_from(&path);
        assert_eq!(loaded.search.max_results, 42);
    }
}
