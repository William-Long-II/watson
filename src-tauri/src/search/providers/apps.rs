//! Indexed-application provider.
//!
//! Emits one `SearchResult` per indexed app. Fuzzy matching against
//! the query happens downstream in `SearchEngine::search` — the
//! provider's job is to package every app with its frecency bonus
//! and let the engine rank them.
//!
//! Why we don't filter here: the existing dispatcher relies on the
//! engine's fuzzy scoring being consistent across all result kinds
//! (apps, snippets, web searches, …). Doing a partial filter inside
//! the provider would either bypass that ranking or duplicate it.
//! The downstream cost is bounded — apps are typically ≤ a few
//! hundred entries.
//!
//! The dispatcher gates calling this provider on
//! `!query.contains(' ') || items.is_empty()` (single-word query or
//! "fall back when nothing else matched"). That perf trim stays in
//! the dispatcher; this provider doesn't peek at items count.

use crate::db::AppEntry;
use crate::search::provider::ResultProvider;
use crate::search::ranking::frecency_score;
use crate::search::{ResultType, SearchAction, SearchResult};

pub struct AppsProvider<'a> {
    pub apps: &'a [AppEntry],
    /// When false, all apps emit `frecency_score: 0.0` and rank purely
    /// by fuzzy match score. Mirrors the `settings.search
    /// .use_frequency_ranking` switch.
    pub use_frequency_ranking: bool,
    /// Current Unix timestamp; passed in so the provider stays pure
    /// (no clock reads inside the trait) and tests can pin time.
    pub now: i64,
}

impl<'a> ResultProvider for AppsProvider<'a> {
    fn name(&self) -> &'static str {
        "apps"
    }

    fn search(&self, _query: &str) -> Vec<SearchResult> {
        // The query is intentionally ignored here. Fuzzy ranking lives
        // in `SearchEngine::search` downstream and handles all result
        // kinds uniformly; if we filtered here we'd skip the engine's
        // scoring and break the relative ordering between apps and
        // other result kinds.
        self.apps
            .iter()
            .map(|app| {
                let bonus = if self.use_frequency_ranking {
                    frecency_score(app.launch_count, app.last_launched, self.now)
                } else {
                    0.0
                };
                SearchResult {
                    id: app.id.clone(),
                    name: app.name.clone(),
                    description: "Application".to_string(),
                    icon: app.icon_cache_path.clone(),
                    result_type: ResultType::Application,
                    score: 0,
                    frecency_score: bonus,
                    preview: None,
                    pinned: false,
                    action: SearchAction::LaunchApp {
                        path: app.path.clone(),
                    },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, launches: i32, last: Option<i64>) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: name.into(),
            path: format!("/Applications/{name}.app"),
            icon_cache_path: None,
            launch_count: launches,
            last_launched: last,
            platform: "test".into(),
            modified_at: 0,
        }
    }

    #[test]
    fn empty_apps_returns_empty() {
        let p = AppsProvider {
            apps: &[],
            use_frequency_ranking: true,
            now: 0,
        };
        assert!(p.search("anything").is_empty());
    }

    #[test]
    fn maps_each_app_to_one_search_result() {
        let apps = vec![
            app("a:1", "Brave", 0, None),
            app("a:2", "Chrome", 0, None),
            app("a:3", "Firefox", 0, None),
        ];
        let p = AppsProvider {
            apps: &apps,
            use_frequency_ranking: false,
            now: 0,
        };
        let results = p.search("");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "Brave");
        assert_eq!(results[2].name, "Firefox");
    }

    #[test]
    fn frequency_off_zeroes_the_bonus() {
        let apps = vec![app("a:1", "Brave", 999, Some(0))];
        let p = AppsProvider {
            apps: &apps,
            use_frequency_ranking: false,
            now: 1_700_000_000,
        };
        let r = &p.search("")[0];
        assert_eq!(r.frecency_score, 0.0);
    }

    #[test]
    fn frequency_on_emits_nonzero_bonus_for_used_apps() {
        let apps = vec![
            app("a:never", "Never used", 0, None),
            app("a:hot", "Frequently used", 50, Some(1_699_990_000)),
        ];
        let p = AppsProvider {
            apps: &apps,
            use_frequency_ranking: true,
            now: 1_700_000_000,
        };
        let results = p.search("");
        // Unused app gets 0; hot app gets a positive score.
        assert_eq!(results[0].frecency_score, 0.0);
        assert!(results[1].frecency_score > 0.0);
    }

    #[test]
    fn search_action_is_launch_app_with_path() {
        let apps = vec![app("a:1", "Brave", 0, None)];
        let p = AppsProvider {
            apps: &apps,
            use_frequency_ranking: false,
            now: 0,
        };
        let r = &p.search("")[0];
        assert!(matches!(
            r.action,
            SearchAction::LaunchApp { ref path } if path.contains("Brave")
        ));
    }

    #[test]
    fn provider_name_is_stable() {
        let p = AppsProvider {
            apps: &[],
            use_frequency_ranking: false,
            now: 0,
        };
        assert_eq!(p.name(), "apps");
    }
}
