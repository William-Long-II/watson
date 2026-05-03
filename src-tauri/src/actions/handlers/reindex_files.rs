//! Handler for `SearchAction::ReindexFiles`.
//!
//! Triggered by the empty-state "Re-index files now" affordance the
//! `f` route surfaces. Validates that file search is enabled in
//! settings (rejecting with a typed error if not), resets the
//! indexer's cancellation token, builds a fresh `FileIndexer` from
//! the configured paths / exclusions / max depth, and runs a full
//! sweep.
//!
//! Lifetime: takes the `&FileSearchManager` (Arc-cloned for the
//! indexer) plus the snapshot of settings the indexer needs. The
//! caller (`execute_action`) holds the settings RwLock briefly to
//! pull these values; cloning is cheap (Vec<String> + a couple of
//! primitives) and avoids holding the guard across the indexer's
//! work.

use std::sync::Arc;

use crate::config::settings::FileSearchSettings;
use crate::files::indexer::FileIndexer;
use crate::files::FileSearchManager;

pub fn handle(
    file_search: Arc<FileSearchManager>,
    file_search_settings: FileSearchSettings,
) -> Result<(), String> {
    if !file_search_settings.enabled {
        return Err("File search is disabled in Settings".to_string());
    }
    file_search.reset_cancel();
    let indexer = FileIndexer::new(
        file_search,
        file_search_settings.indexed_paths,
        file_search_settings.excluded_patterns,
        file_search_settings.max_depth,
    );
    indexer.index_all();
    Ok(())
}
