pub mod dispatch;
pub mod ranking;
pub mod url_builder;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub result_type: ResultType,
    pub score: i64,
    /// Gemini Improvement #2: secondary sort key. Items carry their
    /// decayed usage-frequency (frecency) here. When two results share
    /// the same fuzzy `score`, the higher `frecency_score` wins.
    /// Zero-default so construction sites only opt-in.
    #[serde(default, rename = "usage_bonus", skip_serializing_if = "is_zero_f64")]
    pub frecency_score: f64,
    /// Gemini Improvement #4: optional plain-text snippet for results that
    /// support a preview (like notes or snippets).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// WAT-303: populated only for clipboard results — indicates the
    /// entry is pinned (survives clear-history; sorts above unpinned).
    /// Default false, skip-serialize-if-false so non-clipboard results
    /// don't bloat the IPC payload.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pinned: bool,
    pub action: SearchAction,
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultType {
    Application,
    WebSearch,
    SystemCommand,
    Clipboard,
    Note,
    File,
    /// WAT-203: inline calculation result (arithmetic, unit conversion,
    /// currency). The action is always `CopyClipboard { content: result }`.
    Calculation,
    /// WAT-301: user-defined text snippet. The action is always
    /// `PasteSnippet { expansion }` — copy to clipboard then synthesize
    /// a paste into the prior-focused window.
    Snippet,
    /// Gemini Improvement #6: an open window that can be switched to.
    OpenWindow,
    /// A specific browser tab inside an open browser window. Surfaced
    /// via UIA enumeration of the tab strip; selecting one invokes
    /// `SelectionItemPattern.Select()` on the underlying TabItem and
    /// brings the parent window to front.
    BrowserTab,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchAction {
    LaunchApp { path: String },
    OpenUrl { url: String },
    RunCommand { command: String },
    CopyClipboard { content: String },
    OpenNote { note_id: String },
    OpenFile { path: String },
    /// WAT-301: copy `expansion` to the clipboard and synthesize a
    /// paste into the prior-focused window.
    PasteSnippet { expansion: String },
    /// Focus an existing top-level window by HWND. (HWND is a pointer
    /// serialized as i64 — fits in JSON's safe-integer range because
    /// user-mode HWNDs stay under 2^53 on Windows.)
    FocusWindow { hwnd: i64 },
    /// Switch to a specific browser tab inside an open browser window.
    /// `hwnd` is the parent window; `index` is the tab's position in
    /// the strip at enumeration time.
    FocusBrowserTab { hwnd: i64, index: i32 },
    /// Open the new-note editor with an empty buffer. Replaces the
    /// removed bare-`n` keyboard shortcut: a "Create new note" entry
    /// surfaces in the notes-search route so users can still reach
    /// the editor from the launcher.
    CreateNewNote,
}

pub struct SearchEngine {
    matcher: SkimMatcherV2,
}

/// WAT-202 / R-09: description-only matches ranked below name matches.
///
/// When a query matches an item's description but not its name, we still
/// want to surface it (previous behavior silently dropped the item). A
/// 50% multiplier on the description score keeps the relative ordering
/// within description-only hits while pushing them beneath any name hit
/// of comparable quality. The exact value isn't load-bearing — it just
/// has to be <100% so that name-vs-description ties favor the name.
const DESCRIPTION_SCORE_PENALTY_PERCENT: i64 = 50;

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn score(&self, query: &str, target: &str) -> Option<i64> {
        self.matcher.fuzzy_match(target, query)
    }

    /// Score an item against the query, considering both `name` and
    /// `description`. Returns the best of the two (with description
    /// penalized) or `None` if neither matches.
    fn score_item(&self, query: &str, name: &str, description: &str) -> Option<i64> {
        let name_score = self.score(query, name);
        let description_score = self
            .score(query, description)
            .map(|s| s * DESCRIPTION_SCORE_PENALTY_PERCENT / 100);
        // Option::max returns the higher variant; Some(x).max(None) == Some(x).
        name_score.max(description_score)
    }

    pub fn search(&self, query: &str, items: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = items
            .into_iter()
            .filter_map(|mut item| {
                self.score_item(query, &item.name, &item.description).map(|score| {
                    item.score = score;
                    item
                })
            })
            .collect();

        // Gemini Improvement #2: two-key descending sort — primary
        // `score`, secondary `frecency_score`. When scores differ,
        // frecency is irrelevant (matching the "only breaks ties"
        // contract). When scores are equal, more-used items float up.
        // `partial_cmp` can't return None for finite f64; fall back to
        // Equal defensively.
        results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| {
                    b.frecency_score
                        .partial_cmp(&a.frecency_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        results
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
