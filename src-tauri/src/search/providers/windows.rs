//! Open-window provider.
//!
//! Emits one `SearchResult` per visible top-level window, scored
//! slightly above plain apps so an open window outranks the
//! corresponding "Launch" entry when both match the query (the user
//! typically wants to switch, not re-launch).
//!
//! Lifetime: borrows a slice of already-enumerated windows. The
//! dispatcher in `lib.rs::search` calls
//! `actions::windows::get_open_windows()` once per query and passes
//! the slice to BOTH this provider and `BrowserTabsProvider`, so the
//! cross-platform FFI work doesn't run twice.

use crate::actions::windows::WindowEntry;
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Score open windows land on. Above the apps floor (0) so when an
/// app is already open we surface "switch to it" rather than
/// "launch a second copy".
const OPEN_WINDOW_SCORE: i64 = 100;

pub struct WindowsProvider<'a> {
    pub windows: &'a [WindowEntry],
}

#[async_trait::async_trait]
impl<'a> ResultProvider for WindowsProvider<'a> {
    fn name(&self) -> &'static str {
        "windows"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        // Query intentionally ignored — fuzzy ranking lives in
        // `SearchEngine::search` downstream and handles all kinds
        // uniformly.
        self.windows
            .iter()
            .map(|window| SearchResult {
                // HWND is unique per window; using it as the id means
                // multi-window apps (browser with three windows,
                // editor with two projects) get distinct switcher rows
                // instead of collapsing into one.
                id: format!("win:{:x}", window.hwnd),
                name: window.title.clone(),
                description: format!("Switch to {}", window.process_name),
                icon: Some("window".to_string()),
                result_type: ResultType::OpenWindow,
                score: OPEN_WINDOW_SCORE,
                frecency_score: 0.0,
                preview: None,
                pinned: false,
                action: SearchAction::FocusWindow { hwnd: window.hwnd },
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(hwnd: i64, title: &str, process: &str) -> WindowEntry {
        WindowEntry {
            hwnd,
            pid: 0,
            process_name: process.into(),
            title: title.into(),
        }
    }

    #[tokio::test]
    async fn empty_windows_returns_empty() {
        let p = WindowsProvider { windows: &[] };
        assert!(p.search("").await.is_empty());
    }

    #[tokio::test]
    async fn each_window_becomes_one_result_with_focus_window_action() {
        let windows = vec![
            win(0x100, "Project A — VS Code", "code"),
            win(0x200, "Inbox — Mail", "mail"),
        ];
        let p = WindowsProvider { windows: &windows };
        let results = p.search("").await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Project A — VS Code");
        assert!(matches!(
            results[0].action,
            SearchAction::FocusWindow { hwnd } if hwnd == 0x100
        ));
        assert_eq!(results[1].id, "win:200");
    }

    #[tokio::test]
    async fn ids_are_distinct_for_different_hwnds_with_same_title() {
        // Multi-window apps (browser with two windows of the same
        // page title) need distinct ids or the listbox de-dupes them
        // against each other.
        let windows = vec![
            win(0x100, "GitHub", "browser"),
            win(0x101, "GitHub", "browser"),
        ];
        let p = WindowsProvider { windows: &windows };
        let results = p.search("").await;
        assert_eq!(results[0].id, "win:100");
        assert_eq!(results[1].id, "win:101");
        assert_ne!(results[0].id, results[1].id);
    }

    #[tokio::test]
    async fn description_uses_process_name() {
        let windows = vec![win(0x1, "Whatever", "Custom Process")];
        let p = WindowsProvider { windows: &windows };
        assert_eq!(
            p.search("").await[0].description,
            "Switch to Custom Process"
        );
    }

    #[test]
    fn provider_name_is_stable() {
        let p = WindowsProvider { windows: &[] };
        assert_eq!(p.name(), "windows");
    }
}
