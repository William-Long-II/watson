//! Unified-captures provider for the `cap` / `captures` route.
//!
//! Aggregates over the three capture-shaped managers (notes, snippets,
//! clipboard) and returns a single, time-sorted list. Listing mode
//! pulls the most-recently-touched N from each, merges, and trims to
//! `MAX_CAPTURE_RESULTS`. Search mode runs each manager's own search
//! against the substring and merges the same way.
//!
//! Sort key: `modified_at` desc — recency is the most useful default
//! when "what was I just working on?" is the question. Within a tie
//! we don't impose a kind preference; the merge is stable so the
//! manager iteration order (notes, snippets, clipboard) breaks ties
//! quietly.

use crate::capture::{capture_action, Capture, CaptureKind, CaptureView};
use crate::clipboard::ClipboardManager;
use crate::notes::NotesManager;
use crate::search::dispatch::SubQuery;
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};
use crate::snippets::SnippetsManager;

/// Score captures land on. Same as the notes route — captures is an
/// exclusive route so apps/web-search aren't mixed in, but the score
/// floor matters for the ResultsList comparator's secondary fallback.
const CAPTURE_SCORE: i64 = 10_000;

/// Per-manager pull cap during listing mode. Three managers × 8 = up
/// to 24 captures pre-merge, post-merge trimmed to MAX_CAPTURE_RESULTS.
const PER_KIND_LIMIT: usize = 8;

/// Final result-list cap shown to the user.
const MAX_CAPTURE_RESULTS: usize = 12;

/// Single-line preview of a capture's content for the result row.
/// Strips markdown headers, collapses newlines, truncates with an
/// ellipsis. Intentionally identical in spirit to `note_preview` /
/// `snippet_preview`; keeping it inline here avoids cross-provider
/// imports and lets the captures route apply uniform truncation.
fn capture_preview(content: &str) -> String {
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

/// Description prefix per kind. Surfaces "Note · …", "Snippet · …",
/// "Clipboard · …" so the user can see at a glance which subsystem
/// the row came from.
fn kind_label(kind: CaptureKind) -> &'static str {
    match kind {
        CaptureKind::Note => "Note",
        CaptureKind::Snippet => "Snippet",
        CaptureKind::Clipboard => "Clipboard",
    }
}

pub struct CapturesProvider<'a> {
    pub notes: &'a NotesManager,
    pub snippets: &'a SnippetsManager,
    pub clipboard: &'a ClipboardManager,
    pub sub: SubQuery,
}

#[async_trait::async_trait]
impl<'a> ResultProvider for CapturesProvider<'a> {
    fn name(&self) -> &'static str {
        "captures"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        let views = match &self.sub {
            SubQuery::Listing => self.collect_recent().await,
            SubQuery::Search(q) if q.is_empty() => self.collect_recent().await,
            SubQuery::Search(q) => self.collect_matches(q).await,
        };

        views.into_iter().map(view_to_result).collect()
    }
}

impl<'a> CapturesProvider<'a> {
    async fn collect_recent(&self) -> Vec<CaptureView> {
        let mut views: Vec<CaptureView> = Vec::new();

        if let Ok(notes) = self.notes.get_recent(PER_KIND_LIMIT).await {
            views.extend(notes.iter().map(note_to_view));
        }
        if let Ok(snippets) = self.snippets.list() {
            views.extend(snippets.iter().take(PER_KIND_LIMIT).map(snippet_to_view));
        }
        let clips = self.clipboard.get_history();
        views.extend(clips.iter().take(PER_KIND_LIMIT).map(clip_to_view));

        sort_and_trim(views)
    }

    async fn collect_matches(&self, query: &str) -> Vec<CaptureView> {
        let mut views: Vec<CaptureView> = Vec::new();

        if let Ok(notes) = self.notes.search(query).await {
            views.extend(notes.iter().map(note_to_view));
        }
        if let Ok(snippets) = self.snippets.search(query) {
            views.extend(snippets.iter().map(snippet_to_view));
        }
        let clips = self.clipboard.search_history(query);
        views.extend(clips.iter().map(clip_to_view));

        sort_and_trim(views)
    }
}

fn note_to_view(n: &crate::notes::Note) -> CaptureView {
    CaptureView {
        id: n.id().to_string(),
        kind: n.kind(),
        title: n.title().to_string(),
        content: n.content().to_string(),
        created_at: n.created_at(),
        modified_at: n.modified_at(),
        tags: n.tags(),
    }
}

fn snippet_to_view(s: &crate::snippets::Snippet) -> CaptureView {
    CaptureView {
        id: s.id().to_string(),
        kind: s.kind(),
        title: s.title().to_string(),
        content: s.content().to_string(),
        created_at: s.created_at(),
        modified_at: s.modified_at(),
        tags: s.tags(),
    }
}

fn clip_to_view(c: &crate::clipboard::ClipboardEntry) -> CaptureView {
    CaptureView {
        id: c.id().to_string(),
        kind: c.kind(),
        title: c.title().to_string(),
        content: c.content().to_string(),
        created_at: c.created_at(),
        modified_at: c.modified_at(),
        tags: c.tags(),
    }
}

fn sort_and_trim(mut views: Vec<CaptureView>) -> Vec<CaptureView> {
    // Stable sort so equal modified_ats preserve insertion order
    // (notes → snippets → clipboard). Reverse for desc.
    views.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    views.truncate(MAX_CAPTURE_RESULTS);
    views
}

fn view_to_result(view: CaptureView) -> SearchResult {
    let preview = capture_preview(&view.content);
    let description = if view.tags.is_empty() {
        format!("{} · {}", kind_label(view.kind), preview)
    } else {
        format!("{} · {}", kind_label(view.kind), view.tags.join(", "))
    };
    let action = capture_action(&view);
    SearchResult {
        id: view.id.clone(),
        name: view.title,
        description,
        icon: Some(view.kind.icon().to_string()),
        result_type: capture_result_type(view.kind),
        score: CAPTURE_SCORE,
        frecency_score: 0.0,
        preview: Some(preview),
        pinned: false,
        action,
    }
}

fn capture_result_type(kind: CaptureKind) -> ResultType {
    match kind {
        CaptureKind::Note => ResultType::Note,
        CaptureKind::Snippet => ResultType::Snippet,
        CaptureKind::Clipboard => ResultType::Clipboard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(kind: CaptureKind, id: &str, modified_at: i64) -> CaptureView {
        CaptureView {
            id: id.into(),
            kind,
            title: format!("title-{id}"),
            content: format!("content-{id}"),
            created_at: modified_at - 100,
            modified_at,
            tags: vec![],
        }
    }

    #[test]
    fn sort_and_trim_orders_by_modified_at_desc() {
        let views = vec![
            view(CaptureKind::Note, "a", 100),
            view(CaptureKind::Snippet, "b", 300),
            view(CaptureKind::Clipboard, "c", 200),
        ];
        let sorted = sort_and_trim(views);
        let ids: Vec<String> = sorted.iter().map(|v| v.id.clone()).collect();
        assert_eq!(ids, vec!["b".to_string(), "c".to_string(), "a".to_string()]);
    }

    #[test]
    fn sort_and_trim_caps_at_max_results() {
        // Ensure the trim runs after the sort so the OLDEST entries
        // get dropped, never the most-recently-touched ones.
        let mut views = Vec::new();
        for i in 0..(MAX_CAPTURE_RESULTS as i64 + 5) {
            views.push(view(CaptureKind::Note, &format!("n{i}"), i));
        }
        let sorted = sort_and_trim(views);
        assert_eq!(sorted.len(), MAX_CAPTURE_RESULTS);
        // Newest (highest modified_at) survives.
        assert_eq!(sorted.first().unwrap().modified_at, MAX_CAPTURE_RESULTS as i64 + 4);
        // Oldest five got dropped.
        let oldest_kept = sorted.last().unwrap().modified_at;
        assert!(oldest_kept >= 5);
    }

    #[test]
    fn view_to_result_uses_kind_for_icon_and_result_type() {
        let v = view(CaptureKind::Snippet, "snip:1", 0);
        let r = view_to_result(v);
        assert_eq!(r.icon.as_deref(), Some("snippet"));
        assert!(matches!(r.result_type, ResultType::Snippet));
    }

    #[test]
    fn view_to_result_action_routes_per_kind() {
        let note_view = view(CaptureKind::Note, "note:1", 0);
        let r = view_to_result(note_view);
        assert!(matches!(r.action, SearchAction::OpenNote { .. }));

        let clip_view = view(CaptureKind::Clipboard, "clip:1", 0);
        let r = view_to_result(clip_view);
        assert!(matches!(r.action, SearchAction::CopyClipboard { .. }));

        let snip_view = view(CaptureKind::Snippet, "snip:1", 0);
        let r = view_to_result(snip_view);
        assert!(matches!(r.action, SearchAction::PasteSnippet { .. }));
    }

    #[test]
    fn view_to_result_description_uses_tags_when_present_else_preview() {
        let mut v = view(CaptureKind::Note, "n", 0);
        v.tags = vec!["work".into(), "urgent".into()];
        let with_tags = view_to_result(v);
        assert!(with_tags.description.contains("work, urgent"));

        let v2 = view(CaptureKind::Note, "n2", 0);
        let no_tags = view_to_result(v2);
        // Falls back to a preview snippet of the content.
        assert!(no_tags.description.contains("content-n2"));
    }

    #[test]
    fn capture_preview_strips_markdown_headers() {
        let content = "# Heading\nSome content.\n## Sub\nMore text.";
        let preview = capture_preview(content);
        assert!(!preview.contains("#"));
        assert!(preview.contains("Some content."));
    }

    #[test]
    fn capture_preview_truncates_long_content_with_ellipsis() {
        let long: String = "x".repeat(150);
        let preview = capture_preview(&long);
        assert_eq!(preview.chars().count(), 101);
        assert!(preview.ends_with('…'));
    }

    // Provider-level integration (live managers) is exercised through
    // the existing notes / snippets / clipboard route tests. Adding a
    // duplicate harness here would re-test the underlying managers
    // without exercising new code paths.
}
