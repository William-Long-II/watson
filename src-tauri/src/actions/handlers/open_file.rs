//! Handler for `SearchAction::OpenFile`.
//!
//! Records that the user opened the file (frecency for the file
//! search index) BEFORE invoking the OS opener. Recording is
//! best-effort — a failed write is logged but the open still
//! proceeds. Same intent-vs-success policy as `launch_app`.
//!
//! Lifetime: takes a `&FileSearchManager` reference for the
//! recording side effect. The OS-level `open::that` is platform-
//! provided and needs no app state.

use crate::files::FileSearchManager;

pub fn handle(path: String, file_search: &FileSearchManager) -> Result<(), String> {
    // WAT-304: record the open before invoking the OS so stats
    // reflect user intent even if the OS-level open fails.
    // Failure to record is non-fatal — the open proceeds.
    let _ = file_search.record_open(&path);
    open::that(&path).map_err(|e| e.to_string())
}
