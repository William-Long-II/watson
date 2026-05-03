//! Indexed-files provider for the `f` route.
//!
//! Returns recently-opened files when the route is summoned with no
//! search term, or substring matches when a term is provided. Always
//! appends a "Re-index files now" affordance row at the bottom so a
//! user with an empty file index has a discoverable next step.
//!
//! `FileSearchManager` is sync (rusqlite's blocking API); the
//! provider is async to fit the `ResultProvider` contract but
//! contains no `.await` — the trait async-ness is uniform, the
//! work doesn't have to be.

use crate::files::FileSearchManager;
use crate::search::dispatch::SubQuery;
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Score files land on. Same level as recent notes — both are
/// "user content the route surfaced explicitly".
const FILE_SCORE: i64 = 10_000;
/// "Re-index files now" affordance score — sorts to the bottom
/// when files exist, sole result on a fresh / empty index.
const REINDEX_SCORE: i64 = 1;
/// Cap on how many files surface — matches the prior route
/// implementation.
const FILES_LIMIT: usize = 8;

pub struct FileSearchProvider<'a> {
    pub manager: &'a FileSearchManager,
    pub sub: SubQuery,
}

#[async_trait::async_trait]
impl<'a> ResultProvider for FileSearchProvider<'a> {
    fn name(&self) -> &'static str {
        "file_search"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        let files = match &self.sub {
            SubQuery::Listing => self.manager.get_recent(FILES_LIMIT),
            SubQuery::Search(q) if q.is_empty() => self.manager.get_recent(FILES_LIMIT),
            SubQuery::Search(q) => self.manager.search(q, FILES_LIMIT),
        };

        let mut out: Vec<SearchResult> = files
            .map(|files| {
                files
                    .into_iter()
                    .map(|file| SearchResult {
                        id: file.id.clone(),
                        name: file.name,
                        description: file.path.clone(),
                        icon: Some("file".to_string()),
                        result_type: ResultType::File,
                        score: FILE_SCORE,
                        frecency_score: 0.0,
                        preview: None,
                        pinned: false,
                        action: SearchAction::OpenFile { path: file.path },
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Always surface a "Re-index files now" affordance so users
        // with an empty file index have a discoverable path forward.
        out.push(SearchResult {
            id: "file:__reindex__".to_string(),
            name: "Re-index files now".to_string(),
            description: "Scan the configured paths for files".to_string(),
            icon: Some("file_reindex".to_string()),
            result_type: ResultType::File,
            score: REINDEX_SCORE,
            frecency_score: 0.0,
            preview: None,
            pinned: false,
            action: SearchAction::ReindexFiles,
        });
        out
    }
}

// Provider behavior tests are limited because FileSearchManager is
// DB-backed. The existing route integration tests cover the
// recents / search / reindex-affordance behavior end-to-end.
