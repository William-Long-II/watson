#[cfg(test)]
mod tests {
    use crate::search::{ResultType, SearchAction, SearchEngine, SearchResult};

    fn item(name: &str) -> SearchResult {
        SearchResult {
            id: format!("test:{name}"),
            name: name.to_string(),
            description: "Test App".into(),
            icon: None,
            result_type: ResultType::Application,
            score: 0,
            action: SearchAction::LaunchApp {
                path: "/app".into(),
            },
        }
    }

    #[test]
    fn exact_prefix_scores_at_least_as_high_as_longer_match() {
        let engine = SearchEngine::new();
        let chrome = engine.score("chr", "Chrome").unwrap();
        let chromium = engine.score("chr", "Chromium").unwrap();
        assert!(
            chrome >= chromium,
            "Chrome ({chrome}) should score >= Chromium ({chromium}) for query 'chr'"
        );
    }

    #[test]
    fn no_match_returns_none() {
        let engine = SearchEngine::new();
        assert!(engine.score("xyz", "Chrome").is_none());
    }

    #[test]
    fn search_filters_out_non_matching_items() {
        let engine = SearchEngine::new();
        let items = vec![item("Chrome"), item("Firefox")];

        let results = engine.search("chr", items);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Chrome");
    }

    #[test]
    fn search_sorts_matching_items_by_score_descending() {
        let engine = SearchEngine::new();
        let items = vec![
            item("Firefox Developer Edition"),
            item("Firefox"),
            item("FooFirefoxBar"),
        ];

        let results = engine.search("firefox", items);

        assert_eq!(results.len(), 3);
        for pair in results.windows(2) {
            assert!(
                pair[0].score >= pair[1].score,
                "results must be sorted by score descending: {} ({}) < {} ({})",
                pair[0].name,
                pair[0].score,
                pair[1].name,
                pair[1].score,
            );
        }
    }
}
