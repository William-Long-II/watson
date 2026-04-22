#[cfg(test)]
mod tests {
    use crate::config::settings::Settings;
    use std::collections::HashSet;

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
}
