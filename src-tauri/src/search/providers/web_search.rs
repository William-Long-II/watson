//! Web-search keyword provider.
//!
//! Surfaces a result when the query starts with a configured web-
//! search keyword (e.g. `g rust async` matches Google's `g` and
//! produces an "Open in Google" row). Classification + URL building
//! reuse the existing helpers in `search::dispatch` and
//! `search::url_builder`; this provider just packages them as a
//! `ResultProvider` impl so the dispatcher in `lib.rs::search` can
//! drop the bespoke struct-literal block.
//!
//! Lifetime: borrows a slice of `WebSearch` configs for one call's
//! duration. Constructed inside `lib.rs::search` per query.

use crate::config::settings::WebSearch;
use crate::search::dispatch::{match_web_search, WebSearchMatch};
use crate::search::provider::ResultProvider;
use crate::search::url_builder::build_web_search_url;
use crate::search::{ResultType, SearchAction, SearchResult};

/// The numeric score web-search results carry into the dispatcher.
/// Above the recents-bonus floor (10000) so a known keyword surfaces
/// at the top of mixed result lists; below the calculator
/// short-circuit (100000) which doesn't go through this path anyway.
const WEB_SEARCH_SCORE: i64 = 10_000;

pub struct WebSearchProvider<'a> {
    pub configs: &'a [WebSearch],
}

impl<'a> ResultProvider for WebSearchProvider<'a> {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn search(&self, query: &str) -> Vec<SearchResult> {
        let WebSearchMatch::Matched { index, subquery } = match_web_search(query, self.configs)
        else {
            return Vec::new();
        };
        let ws = &self.configs[index];

        // URL construction can fail on bad scheme or missing
        // {instance}; treat both as "skip the result" rather than
        // surfacing an error — the same policy the prior hand-coded
        // path used (lib.rs::search before the refactor).
        let url = match build_web_search_url(&ws.url, ws.instance.as_deref(), &subquery) {
            Ok(u) => u,
            Err(_) => return Vec::new(),
        };

        vec![SearchResult {
            id: format!("web:{}", ws.keyword),
            name: format!("{}: {}", ws.name, subquery),
            description: "Web Search".to_string(),
            icon: ws.icon.clone(),
            result_type: ResultType::WebSearch,
            score: WEB_SEARCH_SCORE,
            frecency_score: 0.0,
            preview: None,
            pinned: false,
            action: SearchAction::OpenUrl { url },
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str, keyword: &str, url: &str) -> WebSearch {
        WebSearch {
            name: name.into(),
            keyword: keyword.into(),
            url: url.into(),
            icon: None,
            requires_setup: false,
            instance: None,
        }
    }

    #[test]
    fn no_match_returns_empty() {
        let configs = vec![cfg("Google", "g", "https://google.com/?q={query}")];
        let p = WebSearchProvider { configs: &configs };
        assert!(p.search("hello world").is_empty());
    }

    #[test]
    fn match_returns_one_result_with_open_url_action() {
        let configs = vec![cfg("Google", "g", "https://google.com/?q={query}")];
        let p = WebSearchProvider { configs: &configs };
        let results = p.search("g rust async");
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.id, "web:g");
        assert_eq!(r.name, "Google: rust async");
        assert!(matches!(
            r.action,
            SearchAction::OpenUrl { ref url } if url.contains("rust")
        ));
    }

    #[test]
    fn malformed_url_template_is_silently_skipped() {
        // build_web_search_url rejects non-http(s) schemes — the prior
        // path (and this provider) treats that as "no result", not as
        // an error to surface.
        let configs = vec![cfg("Bad", "b", "javascript:alert({query})")];
        let p = WebSearchProvider { configs: &configs };
        assert!(p.search("b foo").is_empty());
    }

    #[test]
    fn provider_name_is_stable() {
        let p = WebSearchProvider { configs: &[] };
        assert_eq!(p.name(), "web_search");
    }
}
