//! Capture model — Phase 1A.
//!
//! Notes, snippets, and clipboard entries are three of Watson's "user
//! text" features that previously had zero shared abstraction. Each
//! had its own manager with its own listing/search shape; the new
//! `cap`/`captures` route would have meant three bespoke traversals.
//!
//! `Capture` is the unifying interface: every captured-text entry
//! exposes `id / kind / title / content / created_at / modified_at /
//! tags`. The dispatcher iterates a single `&[Box<dyn Capture>]`-shaped
//! aggregator instead of switching on kind.
//!
//! Scratchpad is intentionally absent: per the Phase 1A cuts in
//! DESIGN.md, scratchpad is folding into "untitled note", which means
//! the existing `Note` impl will cover it once the migration lands.
//! Adding a transient `CaptureKind::Scratchpad` here would be
//! abstraction we'd then have to remove.

use serde::{Deserialize, Serialize};

use crate::clipboard::ClipboardEntry;
use crate::notes::Note;
use crate::search::SearchAction;
use crate::snippets::Snippet;

/// Discriminator for the underlying entry type. The provider uses
/// this to pick the right `SearchAction` and icon when it materializes
/// a `Capture` into a `SearchResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Note,
    Snippet,
    Clipboard,
}

impl CaptureKind {
    /// Stable icon slug the frontend resolves to a glyph.
    pub fn icon(&self) -> &'static str {
        match self {
            CaptureKind::Note => "note",
            CaptureKind::Snippet => "snippet",
            CaptureKind::Clipboard => "clipboard",
        }
    }
}

/// Common read-shape for any captured text entry. Implementations
/// borrow from the underlying entry struct; the provider clones into
/// a `CaptureView` only when it needs to cross an ownership boundary.
pub trait Capture {
    fn id(&self) -> &str;
    fn kind(&self) -> CaptureKind;
    /// Human-readable label for the result row. For notes this is the
    /// note title; for snippets the trigger; for clipboard entries a
    /// trimmed one-line synopsis of the content.
    fn title(&self) -> &str;
    /// The full captured text. Used for substring matching in the
    /// provider's search path and for the result row's preview.
    fn content(&self) -> &str;
    fn created_at(&self) -> i64;
    fn modified_at(&self) -> i64;
    /// Free-form tags. Empty for entry types that don't carry tags
    /// (snippets, clipboard).
    fn tags(&self) -> Vec<String>;
}

/// Concrete, owned, serializable form of a `Capture`. The provider
/// aggregates entries into `CaptureView`s so it can sort and ship a
/// single `Vec<SearchResult>` back to the dispatcher without holding
/// borrows across the async boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureView {
    pub id: String,
    pub kind: CaptureKind,
    pub title: String,
    pub content: String,
    pub created_at: i64,
    pub modified_at: i64,
    pub tags: Vec<String>,
}

impl Capture for CaptureView {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> CaptureKind { self.kind }
    fn title(&self) -> &str { &self.title }
    fn content(&self) -> &str { &self.content }
    fn created_at(&self) -> i64 { self.created_at }
    fn modified_at(&self) -> i64 { self.modified_at }
    fn tags(&self) -> Vec<String> { self.tags.clone() }
}

impl Capture for Note {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> CaptureKind { CaptureKind::Note }
    fn title(&self) -> &str { &self.title }
    fn content(&self) -> &str { &self.content }
    fn created_at(&self) -> i64 { self.created_at }
    fn modified_at(&self) -> i64 { self.modified_at }
    fn tags(&self) -> Vec<String> { self.tags.clone() }
}

impl Capture for Snippet {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> CaptureKind { CaptureKind::Snippet }
    /// Trigger is the canonical label — it's what the user types to
    /// invoke the snippet, so it's the most-recognizable identifier
    /// in the result row.
    fn title(&self) -> &str { &self.trigger }
    /// The expansion is what gets pasted, which is what the user is
    /// searching the body of when they go looking for a snippet.
    fn content(&self) -> &str { &self.expansion }
    fn created_at(&self) -> i64 { self.created_at }
    fn modified_at(&self) -> i64 { self.modified_at }
    fn tags(&self) -> Vec<String> { Vec::new() }
}

impl Capture for ClipboardEntry {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> CaptureKind { CaptureKind::Clipboard }
    /// Clipboard entries have no first-class title. The stored
    /// `preview` is already a short, single-line synopsis suitable
    /// for the row label.
    fn title(&self) -> &str { &self.preview }
    fn content(&self) -> &str { &self.content }
    fn created_at(&self) -> i64 { self.timestamp.timestamp() }
    fn modified_at(&self) -> i64 { self.timestamp.timestamp() }
    fn tags(&self) -> Vec<String> { Vec::new() }
}

/// Convert a `Capture` into the `SearchAction` that activates it.
/// Each kind reuses the existing per-kind action so the executor
/// path stays untouched.
pub fn capture_action(view: &CaptureView) -> SearchAction {
    match view.kind {
        CaptureKind::Note => SearchAction::OpenNote {
            note_id: view.id.clone(),
        },
        CaptureKind::Snippet => SearchAction::PasteSnippet {
            expansion: view.content.clone(),
        },
        CaptureKind::Clipboard => SearchAction::CopyClipboard {
            content: view.content.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    fn fixed_ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().expect("valid ts")
    }

    fn note(id: &str, title: &str, content: &str) -> Note {
        Note {
            id: id.into(),
            title: title.into(),
            content: content.into(),
            tags: vec!["work".into()],
            created_at: 1_000,
            modified_at: 2_000,
            external_changes: None,
        }
    }

    fn snippet(id: &str, trigger: &str, expansion: &str) -> Snippet {
        Snippet {
            id: id.into(),
            trigger: trigger.into(),
            name: "label".into(),
            expansion: expansion.into(),
            created_at: 1_000,
            modified_at: 2_000,
        }
    }

    fn clip(id: &str, content: &str, preview: &str) -> ClipboardEntry {
        ClipboardEntry {
            id: id.into(),
            content: content.into(),
            preview: preview.into(),
            timestamp: fixed_ts(3_000),
            pinned: false,
        }
    }

    #[test]
    fn note_capture_surface_matches_underlying_fields() {
        let n = note("note:1", "Meeting", "body");
        assert_eq!(n.id(), "note:1");
        assert_eq!(n.kind(), CaptureKind::Note);
        assert_eq!(n.title(), "Meeting");
        assert_eq!(n.content(), "body");
        assert_eq!(n.created_at(), 1_000);
        assert_eq!(n.modified_at(), 2_000);
        assert_eq!(n.tags(), vec!["work".to_string()]);
    }

    #[test]
    fn snippet_uses_trigger_as_title_and_expansion_as_content() {
        // The provider's title/content split has to match what
        // CapturesProvider relies on for substring matching: searching
        // for the trigger should hit `title()`, searching the body
        // should hit `content()`.
        let s = snippet("snip:1", ";addr", "123 Main St");
        assert_eq!(s.title(), ";addr");
        assert_eq!(s.content(), "123 Main St");
        assert_eq!(s.kind(), CaptureKind::Snippet);
        assert!(s.tags().is_empty());
    }

    #[test]
    fn clipboard_uses_preview_as_title_and_timestamp_for_both_dates() {
        // Clipboard entries don't carry a separate modified_at, so
        // both Capture::created_at and Capture::modified_at fall back
        // to the same timestamp. This test pins that behavior — if we
        // ever add an edit-on-pin feature, modified_at will need a
        // new field on ClipboardEntry too.
        let c = clip("clip:1", "https://example.com/long-url", "https://example.com/lo…");
        assert_eq!(c.title(), "https://example.com/lo…");
        assert_eq!(c.content(), "https://example.com/long-url");
        assert_eq!(c.kind(), CaptureKind::Clipboard);
        assert_eq!(c.created_at(), c.modified_at());
    }

    #[test]
    fn capture_view_round_trips_through_clone() {
        let view = CaptureView {
            id: "note:1".into(),
            kind: CaptureKind::Note,
            title: "Title".into(),
            content: "Body".into(),
            created_at: 1,
            modified_at: 2,
            tags: vec!["t".into()],
        };
        assert_eq!(view.id(), "note:1");
        assert_eq!(view.kind(), CaptureKind::Note);
        assert_eq!(view.tags(), vec!["t".to_string()]);
    }

    #[test]
    fn capture_action_dispatches_per_kind() {
        let note_view = CaptureView {
            id: "note:42".into(),
            kind: CaptureKind::Note,
            title: "t".into(),
            content: "c".into(),
            created_at: 0,
            modified_at: 0,
            tags: vec![],
        };
        assert!(matches!(
            capture_action(&note_view),
            SearchAction::OpenNote { note_id } if note_id == "note:42"
        ));

        let snip_view = CaptureView {
            id: "snip:1".into(),
            kind: CaptureKind::Snippet,
            title: ";x".into(),
            content: "expanded".into(),
            created_at: 0,
            modified_at: 0,
            tags: vec![],
        };
        assert!(matches!(
            capture_action(&snip_view),
            SearchAction::PasteSnippet { expansion } if expansion == "expanded"
        ));

        let clip_view = CaptureView {
            id: "clip:1".into(),
            kind: CaptureKind::Clipboard,
            title: "p".into(),
            content: "pasteable".into(),
            created_at: 0,
            modified_at: 0,
            tags: vec![],
        };
        assert!(matches!(
            capture_action(&clip_view),
            SearchAction::CopyClipboard { content } if content == "pasteable"
        ));
    }

    #[test]
    fn icon_slug_per_kind_is_stable() {
        // Icon slugs cross the IPC boundary into the frontend's icon
        // resolver. Pin them so a rename here doesn't silently break
        // the row glyph in production.
        assert_eq!(CaptureKind::Note.icon(), "note");
        assert_eq!(CaptureKind::Snippet.icon(), "snippet");
        assert_eq!(CaptureKind::Clipboard.icon(), "clipboard");
    }
}
