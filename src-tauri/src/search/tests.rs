#[cfg(test)]
mod tests {
    use crate::search::{SearchEngine, SearchResult, ResultType, SearchAction};

    #[test]
    fn test_search_engine_creation() {
        let engine = SearchEngine::new();
        assert!(engine.score("chr", "Chrome").is_some());
    }

    #[test]
    fn test_fuzzy_match_scores() {
        let engine = SearchEngine::new();

        // Exact prefix should score higher
        let chrome_score = engine.score("chr", "Chrome").unwrap();
        let chromium_score = engine.score("chr", "Chromium").unwrap();

        // Both should match
        assert!(chrome_score > 0);
        assert!(chromium_score > 0);
    }

    #[test]
    fn test_no_match_returns_none() {
        let engine = SearchEngine::new();
        assert!(engine.score("xyz", "Chrome").is_none());
    }

    #[test]
    fn test_search_filters_and_sorts() {
        let engine = SearchEngine::new();

        let items = vec![
            SearchResult {
                id: "1".to_string(),
                name: "Chrome".to_string(),
                description: "Browser".to_string(),
                icon: None,
                result_type: ResultType::Application,
                score: 0,
                action: SearchAction::LaunchApp { path: "/app".to_string() },
            },
            SearchResult {
                id: "2".to_string(),
                name: "Firefox".to_string(),
                description: "Browser".to_string(),
                icon: None,
                result_type: ResultType::Application,
                score: 0,
                action: SearchAction::LaunchApp { path: "/app".to_string() },
            },
        ];

        let results = engine.search("chr", items);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Chrome");
    }
}
