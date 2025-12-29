#[cfg(test)]
mod tests {
    use crate::config::settings::Settings;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.activation.hotkey, "Alt+Space");
        assert_eq!(settings.search.max_results, 8);
        assert!(settings.general.launch_at_login);
    }

    #[test]
    fn test_default_web_searches_count() {
        let settings = Settings::default();
        assert!(settings.web_searches.len() >= 6);
    }

    #[test]
    fn test_web_search_keywords_unique() {
        let settings = Settings::default();
        let keywords: Vec<_> = settings.web_searches.iter().map(|w| &w.keyword).collect();
        let unique: std::collections::HashSet<_> = keywords.iter().collect();
        assert_eq!(keywords.len(), unique.len(), "Web search keywords must be unique");
    }
}
