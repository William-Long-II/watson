//! Result-provider trait — Phase 1A substrate.
//!
//! Goal: every searchable thing (apps, files, notes, snippets, windows,
//! tabs, calculator, web searches, system commands, scenes-future,
//! captures-future) becomes one `impl ResultProvider`. The dispatcher
//! in `lib.rs::search` iterates a registry instead of hand-coding a
//! bespoke `SearchResult { ... }` block per kind.
//!
//! This first cut keeps the trait minimal and synchronous. Async
//! providers (notes, file_search) will need a parallel `AsyncResultProvider`
//! or an `async_trait` migration in a follow-up — handling that here
//! would inflate this PR's scope and the existing hand-coded async paths
//! work as-is.
//!
//! Lifetime model: providers borrow whatever app state they need for
//! the duration of one `search()` call. Construct them inside the
//! dispatcher per query, drop after results are collected. No long-
//! lived state in the provider itself; that lives in `AppState`.

use crate::search::SearchResult;

/// A source of search results.
///
/// Implementations should:
/// - Return *their own* matches against the query (filtering / fuzzy
///   ranking). The dispatcher does NOT re-rank across providers; each
///   provider is responsible for the score it puts on its results.
/// - Be cheap to construct — they are built per call.
/// - Not panic on empty / malformed queries; return an empty Vec.
///
/// Async-by-default via `async_trait`. Most providers are pure
/// in-memory transforms and just write `async fn search` returning
/// immediately; the async kinds (notes, file_search) `.await`
/// their backing store. One trait, one dispatcher loop.
#[async_trait::async_trait]
pub trait ResultProvider {
    /// Stable identifier used in logs / diagnostics. Should be a
    /// short snake_case slug ("web_search", "system_command",
    /// "calculator").
    fn name(&self) -> &'static str;

    /// Run the provider against the query, returning its results.
    /// Caller appends these to a combined `Vec<SearchResult>`.
    async fn search(&self, query: &str) -> Vec<SearchResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{ResultType, SearchAction, SearchResult};

    /// Minimal in-test provider: returns one fixed result if the
    /// query equals "match", else empty. Validates the dispatcher
    /// can hold and call providers through a trait object.
    struct StubProvider;

    #[async_trait::async_trait]
    impl ResultProvider for StubProvider {
        fn name(&self) -> &'static str {
            "stub"
        }
        async fn search(&self, query: &str) -> Vec<SearchResult> {
            if query == "match" {
                vec![SearchResult {
                    id: "stub:1".into(),
                    name: "Stub match".into(),
                    description: String::new(),
                    icon: None,
                    result_type: ResultType::SystemCommand,
                    score: 100,
                    frecency_score: 0.0,
                    preview: None,
                    pinned: false,
                    action: SearchAction::RunCommand {
                        command: "noop".into(),
                    },
                }]
            } else {
                Vec::new()
            }
        }
    }

    #[test]
    fn name_is_stable() {
        let p = StubProvider;
        assert_eq!(p.name(), "stub");
        assert_eq!(p.name(), p.name());
    }

    #[tokio::test]
    async fn empty_query_returns_empty_vec_without_panic() {
        let p = StubProvider;
        assert!(p.search("").await.is_empty());
    }

    #[tokio::test]
    async fn match_returns_results() {
        let p = StubProvider;
        let results = p.search("match").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "stub:1");
    }

    #[tokio::test]
    async fn provider_works_through_dyn_dispatch() {
        // The dispatcher holds providers as `Box<dyn ResultProvider>`;
        // verify trait-object dispatch works for async fns too.
        let providers: Vec<Box<dyn ResultProvider>> = vec![Box::new(StubProvider)];
        let mut combined = Vec::new();
        for p in &providers {
            combined.extend(p.search("match").await);
        }
        assert_eq!(combined.len(), 1);
    }
}
