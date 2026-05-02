//! User-snippet provider.
//!
//! Surfaces text-expansion snippets whose trigger or name matches the
//! query. Reuses `SnippetsManager::search` for the underlying lookup
//! (DB-backed, ranks by trigger-prefix → name-substring); this
//! provider just packages the matches as `SearchResult`s.
//!
//! Lifetime: borrows `&SnippetsManager` for one call's duration.
//! Constructed inside `lib.rs::search` per query.

use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};
use crate::snippets::SnippetsManager;

/// Score snippets land on. Sits above web-search (10000-floor for
/// keyword matches) is intentional? No — snippets sit at 8000 so
/// they surface reliably when their trigger matches but don't
/// swamp app launchers when both share the prefix space. Match
/// the prior hand-coded value.
const SNIPPET_SCORE: i64 = 8_000;

/// First-line preview of a snippet's expansion for the description
/// row. Collapses multi-line expansions into a single line and
/// truncates with an ellipsis so long snippets still fit.
fn snippet_preview(expansion: &str) -> String {
    let first_line = expansion.lines().next().unwrap_or("");
    let clipped: String = first_line.chars().take(80).collect();
    if first_line.chars().count() > 80 || expansion.contains('\n') {
        format!("{clipped}…")
    } else {
        clipped
    }
}

pub struct SnippetsProvider<'a> {
    pub manager: &'a SnippetsManager,
}

impl<'a> ResultProvider for SnippetsProvider<'a> {
    fn name(&self) -> &'static str {
        "snippets"
    }

    fn search(&self, query: &str) -> Vec<SearchResult> {
        // SnippetsManager::search returns Result; treat errors as
        // "no results" — same policy the prior hand-coded path used.
        // A real DB error already gets logged via the manager's own
        // error path; surfacing it here would dump a stack trace into
        // the user's search bar.
        let snippets = match self.manager.search(query) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        snippets
            .into_iter()
            .map(|snippet| SearchResult {
                id: snippet.id.clone(),
                // Trigger first so the user can see what to type next
                // time without opening Settings.
                name: format!("{}  —  {}", snippet.trigger, snippet.name),
                description: snippet_preview(&snippet.expansion),
                icon: Some("snippet".to_string()),
                result_type: ResultType::Snippet,
                score: SNIPPET_SCORE,
                frecency_score: 0.0,
                preview: None,
                pinned: false,
                action: SearchAction::PasteSnippet {
                    expansion: snippet.expansion,
                },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_preview_single_short_line_unchanged() {
        assert_eq!(snippet_preview("hello"), "hello");
    }

    #[test]
    fn snippet_preview_long_single_line_truncated_with_ellipsis() {
        let long = "x".repeat(100);
        let preview = snippet_preview(&long);
        assert_eq!(preview.chars().count(), 81); // 80 chars + ellipsis
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn snippet_preview_multiline_keeps_first_line_and_appends_ellipsis() {
        let multi = "first line\nsecond line\nthird";
        assert_eq!(snippet_preview(multi), "first line…");
    }

    #[test]
    fn snippet_preview_empty_input_safe() {
        assert_eq!(snippet_preview(""), "");
    }

    #[test]
    fn snippet_preview_unicode_first_line_truncates_by_chars_not_bytes() {
        // Multi-byte chars (emoji + accents) should count by char,
        // not byte — `chars().take(80)` enforces this. 80 emojis
        // (4 bytes each) shouldn't push the truncation boundary
        // incorrectly.
        let unicode_long: String = std::iter::repeat('🎯').take(100).collect();
        let preview = snippet_preview(&unicode_long);
        assert_eq!(preview.chars().count(), 81); // 80 + ellipsis
    }
}
