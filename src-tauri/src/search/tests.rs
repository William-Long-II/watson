#[cfg(test)]
mod tests {
    use crate::search::{ResultType, SearchAction, SearchEngine, SearchResult};

    fn item(name: &str) -> SearchResult {
        // Default description is a string with no overlap with typical
        // test queries — guarantees existing tests aren't accidentally
        // lifted by the WAT-202 description-match branch.
        item_with_description(name, "zzz-test-fixture")
    }

    fn item_with_description(name: &str, description: &str) -> SearchResult {
        SearchResult {
            id: format!("test:{name}"),
            name: name.to_string(),
            description: description.to_string(),
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

    // --- WAT-202 / R-09: description-matched results ---

    #[test]
    fn description_only_match_surfaces_the_item() {
        // Regression test for R-09: before this fix, items whose match
        // lived in `description` but not `name` were silently filtered
        // out. A user typing "mail" wouldn't find "Thunderbird" even
        // though its description says "Email client".
        let engine = SearchEngine::new();
        let items = vec![item_with_description("Thunderbird", "Email client")];

        let results = engine.search("email", items);

        assert_eq!(results.len(), 1, "description-only match must surface the item");
        assert_eq!(results[0].name, "Thunderbird");
    }

    #[test]
    fn name_match_ranks_above_description_only_match_for_same_query() {
        // Pin the penalty contract: when query matches one item's name
        // and another item's description, the name match comes first.
        let engine = SearchEngine::new();
        let items = vec![
            item_with_description("Gmail Helper", "zzz-unrelated"),
            item_with_description("Thunderbird", "Gmail client"),
        ];

        let results = engine.search("gmail", items);

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].name, "Gmail Helper",
            "name match must rank ahead of description-only match; got order: {:?}",
            results.iter().map(|r| &r.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn when_both_name_and_description_match_name_score_wins() {
        // If name matches, we pick the (unpenalized) name score, not the
        // (penalized) description score. Otherwise items with keyword-
        // stuffed descriptions could overtake their own name match.
        let engine = SearchEngine::new();
        let items = vec![item_with_description("Chrome", "Chrome browser from Google")];

        let results = engine.search("chrome", items);
        assert_eq!(results.len(), 1);

        // The score recorded on the result should equal the name match,
        // not the penalized description match.
        let name_only = SearchEngine::new().score("chrome", "Chrome").unwrap();
        assert_eq!(
            results[0].score, name_only,
            "when name matches, recorded score should be the name score"
        );
    }

    #[test]
    fn neither_name_nor_description_match_filters_the_item() {
        // Belt-and-suspenders regression: adding description matching
        // must not accidentally let non-matching items through.
        let engine = SearchEngine::new();
        let items = vec![item_with_description("Calculator", "arithmetic tool")];
        let results = engine.search("firefox", items);
        assert!(results.is_empty());
    }

    #[test]
    fn description_match_with_empty_description_does_not_panic() {
        // Items can legitimately have empty descriptions (clipboard
        // entries, some note types). Must not panic or score them
        // positively by accident.
        let engine = SearchEngine::new();
        let items = vec![item_with_description("Thing", "")];
        let results = engine.search("xyz", items);
        assert!(results.is_empty());
    }
}
