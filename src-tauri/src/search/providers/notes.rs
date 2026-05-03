//! Notes provider for the `n` route.
//!
//! Returns the user's notes — recent if no search term, fuzzy-match
//! over title/tags/content if a term is provided. Always appends a
//! "Create new note" affordance row at the bottom so users with zero
//! notes still get a discoverable next step (this replaces the
//! removed bare-`n` shortcut).
//!
//! Async because the underlying `NotesManager` runs SQLite I/O on a
//! background thread (Tauri/tokio).

use crate::notes::NotesManager;
use crate::search::dispatch::SubQuery;
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Score recent / matched notes land on. Above the apps floor (0)
/// and the open-window score (100) so notes the user explicitly
/// scoped to via the `n ` prefix dominate.
const NOTE_SCORE: i64 = 10_000;
/// "Create new note" affordance. Lower than every note row so it
/// sorts to the bottom when notes exist, but remains visible (and
/// is the only result) on a fresh install.
const CREATE_NEW_NOTE_SCORE: i64 = 1;
/// Cap on how many notes show in the route — matches the prior
/// hand-coded path's `take(8)`.
const NOTES_LIMIT: usize = 8;

/// Clean plain-text snippet of a note's content. Strips markdown
/// headers and collapses newlines to a single line so the preview
/// row reads cleanly even when the content has heavy formatting.
fn note_preview(content: &str) -> String {
    let clean = content
        .lines()
        .filter(|line| !line.trim().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let clipped: String = clean.chars().take(100).collect();
    if clean.chars().count() > 100 {
        format!("{clipped}…")
    } else {
        clipped
    }
}

pub struct NotesProvider<'a> {
    pub manager: &'a NotesManager,
    /// Pre-parsed sub-query from the route classifier. The provider
    /// uses it to decide between `get_recent` and `search`.
    pub sub: SubQuery,
}

#[async_trait::async_trait]
impl<'a> ResultProvider for NotesProvider<'a> {
    fn name(&self) -> &'static str {
        "notes"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        let notes_res = match &self.sub {
            SubQuery::Listing => self.manager.get_recent(NOTES_LIMIT).await,
            SubQuery::Search(q) if q.is_empty() => self.manager.get_recent(NOTES_LIMIT).await,
            SubQuery::Search(q) => self.manager.search(q).await,
        };

        let mut out: Vec<SearchResult> = notes_res
            .map(|notes| {
                notes
                    .into_iter()
                    .take(NOTES_LIMIT)
                    .map(|note| SearchResult {
                        id: note.id.clone(),
                        name: note.title,
                        description: format!("Note · {}", note.tags.join(", ")),
                        icon: Some("note".to_string()),
                        result_type: ResultType::Note,
                        score: NOTE_SCORE,
                        frecency_score: 0.0,
                        preview: Some(note_preview(&note.content)),
                        pinned: false,
                        action: SearchAction::OpenNote { note_id: note.id },
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Always surface a "Create new note" entry. Replaces the
        // removed bare-`n` keyboard shortcut: a user with zero notes
        // who types `n ` would otherwise see an empty list with no
        // path forward.
        out.push(SearchResult {
            id: "note:__create__".to_string(),
            name: "Create new note".to_string(),
            description: "Open the editor with an empty note".to_string(),
            icon: Some("note_new".to_string()),
            result_type: ResultType::Note,
            score: CREATE_NEW_NOTE_SCORE,
            frecency_score: 0.0,
            preview: None,
            pinned: false,
            action: SearchAction::CreateNewNote,
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_preview_strips_markdown_headers() {
        let content = "# Heading\nSome content.\n## Sub\nMore text.";
        let preview = note_preview(content);
        assert!(!preview.contains("#"));
        assert!(preview.contains("Some content."));
        assert!(preview.contains("More text."));
    }

    #[test]
    fn note_preview_collapses_newlines_to_spaces() {
        let preview = note_preview("line one\nline two\nline three");
        assert_eq!(preview, "line one line two line three");
    }

    #[test]
    fn note_preview_truncates_long_content_with_ellipsis() {
        let long: String = "x".repeat(150);
        let preview = note_preview(&long);
        assert_eq!(preview.chars().count(), 101); // 100 + ellipsis
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn note_preview_empty_input_safe() {
        assert_eq!(note_preview(""), "");
    }

    #[test]
    fn note_preview_only_headers_returns_empty_or_blank() {
        // All-header content collapses to empty after the filter.
        let preview = note_preview("# h1\n## h2\n### h3");
        assert!(preview.trim().is_empty());
    }

    // Provider behavior tests are limited because NotesManager is
    // DB-backed and not easily mockable from a unit test. The route
    // logic (sub-query → get_recent vs search → results +
    // create-new affordance) is exercised by the existing notes
    // route integration tests.
}
