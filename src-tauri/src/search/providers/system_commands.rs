//! System-command provider for the `>` route.
//!
//! Surfaces commands like `lock`, `sleep`, `restart`, `split-left`,
//! `maximize`, etc. when the user types `>` (alone for full listing)
//! or `>foo` (substring filter on aliases or name).
//!
//! Lifetime: takes a `SubQuery` directly via constructor — the
//! dispatcher's route classifier has already split the prefix from
//! the rest of the query, so re-parsing inside the provider would be
//! redundant.
//!
//! Runs *exclusively* on the `>` route — the dispatcher does NOT mix
//! its results with apps / web / snippets. The query parameter to
//! `ResultProvider::search` is intentionally unused; the filter
//! comes from `self.sub`.

use crate::actions::system::get_system_commands;
use crate::search::dispatch::SubQuery;
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Score for system-command rows. Above apps, web, and snippets so
/// commands surface above ambient matches when the user explicitly
/// invoked the `>` route.
const SYSTEM_COMMAND_SCORE: i64 = 5_000;

pub struct SystemCommandsProvider {
    /// Already-parsed sub-query from the route classifier. `Listing`
    /// returns every registered command; `Search(q)` filters by
    /// case-insensitive substring against aliases or display name.
    pub sub: SubQuery,
}

#[async_trait::async_trait]
impl ResultProvider for SystemCommandsProvider {
    fn name(&self) -> &'static str {
        "system_commands"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        let filter = match &self.sub {
            SubQuery::Listing => None,
            SubQuery::Search(q) if q.is_empty() => None,
            SubQuery::Search(q) => Some(q.to_lowercase()),
        };

        get_system_commands()
            .into_iter()
            .filter(|cmd| match &filter {
                None => true,
                Some(needle) => cmd
                    .aliases
                    .iter()
                    .any(|alias| alias.to_lowercase().contains(needle))
                    || cmd.name.to_lowercase().contains(needle.as_str()),
            })
            .map(|cmd| SearchResult {
                id: cmd.id.clone(),
                name: cmd.name.clone(),
                description: cmd.description.clone(),
                icon: Some("system".to_string()),
                result_type: ResultType::SystemCommand,
                score: SYSTEM_COMMAND_SCORE,
                frecency_score: 0.0,
                preview: None,
                pinned: false,
                action: SearchAction::RunCommand { command: cmd.id },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn listing_returns_every_registered_command() {
        let p = SystemCommandsProvider {
            sub: SubQuery::Listing,
        };
        let results = p.search("").await;
        // Every registered command surfaces, no filter applied.
        // The exact count varies as commands are added/removed; we
        // only assert non-empty + every result has the expected
        // shape.
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.score, SYSTEM_COMMAND_SCORE);
            assert!(matches!(r.result_type, ResultType::SystemCommand));
            assert!(matches!(r.action, SearchAction::RunCommand { .. }));
        }
    }

    #[tokio::test]
    async fn empty_search_treated_as_listing() {
        let p_listing = SystemCommandsProvider {
            sub: SubQuery::Listing,
        };
        let p_empty = SystemCommandsProvider {
            sub: SubQuery::Search(String::new()),
        };
        assert_eq!(
            p_listing.search("").await.len(),
            p_empty.search("").await.len()
        );
    }

    #[tokio::test]
    async fn substring_filter_matches_alias_case_insensitively() {
        // "lock" should match the lock command's alias / name no
        // matter the case the user types. Test against a token
        // that's almost certainly in the registered commands.
        let p_lower = SystemCommandsProvider {
            sub: SubQuery::Search("lock".into()),
        };
        let p_upper = SystemCommandsProvider {
            sub: SubQuery::Search("LOCK".into()),
        };
        let lower_results = p_lower.search("").await;
        let upper_results = p_upper.search("").await;
        // Same matches regardless of case.
        assert_eq!(lower_results.len(), upper_results.len());
        // Lock command is in the registered set.
        assert!(!lower_results.is_empty(), "expected at least one lock-related command");
    }

    #[tokio::test]
    async fn nonexistent_token_returns_empty() {
        let p = SystemCommandsProvider {
            sub: SubQuery::Search("zzzz-not-a-command-token".into()),
        };
        assert!(p.search("").await.is_empty());
    }

    #[test]
    fn provider_name_is_stable() {
        let p = SystemCommandsProvider {
            sub: SubQuery::Listing,
        };
        assert_eq!(p.name(), "system_commands");
    }
}
