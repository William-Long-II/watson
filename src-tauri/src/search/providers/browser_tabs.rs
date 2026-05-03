//! Browser-tab provider.
//!
//! For each window in the supplied slice that belongs to a known
//! browser process, queries `actions::browser_tabs::get_browser_tabs`
//! and emits one `SearchResult` per tab. Tab rows score slightly
//! above plain window rows so the more-specific "switch to *this*
//! tab" surfaces ahead of "switch to the browser window" when both
//! match.
//!
//! Lifetime: shares the same `&[WindowEntry]` slice the
//! `WindowsProvider` consumes — the dispatcher enumerates windows
//! once and passes the result to both providers.
//!
//! Unit tests for this provider are intentionally light because the
//! tab-enumeration call is cross-platform FFI (UIA on Windows, AX on
//! macOS, AT-SPI on Linux) and not easily mockable. Behavioral
//! confidence comes from the live captures we ran during the
//! original switcher work and the existing tab-enum tests inside
//! `actions::browser_tabs`.

use crate::actions::{browser_tabs, windows::WindowEntry};
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Score tabs land on. Above the open-window score (100) so the
/// tab-level row surfaces ahead of the window-level row when both
/// match the query.
const BROWSER_TAB_SCORE: i64 = 200;

pub struct BrowserTabsProvider<'a> {
    pub windows: &'a [WindowEntry],
}

#[async_trait::async_trait]
impl<'a> ResultProvider for BrowserTabsProvider<'a> {
    fn name(&self) -> &'static str {
        "browser_tabs"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        for window in self.windows {
            // Skip non-browser windows up front — the FFI call to
            // get_browser_tabs is non-trivial (~50ms on a cold tree
            // walk) and yields nothing useful for non-browsers.
            if !browser_tabs::is_browser_process(&window.process_name) {
                continue;
            }
            let tabs = match browser_tabs::get_browser_tabs(window.hwnd, &window.process_name) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for tab in tabs {
                results.push(SearchResult {
                    // Composite id: window HWND + tab index. Stable
                    // for React keying within a single enumeration.
                    id: format!("tab:{:x}:{}", tab.window_hwnd, tab.index),
                    name: tab.name.clone(),
                    description: format!("Tab in {}", tab.process_name),
                    icon: Some("browser_tab".to_string()),
                    result_type: ResultType::BrowserTab,
                    score: BROWSER_TAB_SCORE,
                    frecency_score: 0.0,
                    preview: None,
                    pinned: false,
                    action: SearchAction::FocusBrowserTab {
                        hwnd: tab.window_hwnd,
                        index: tab.index,
                    },
                });
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_windows_returns_empty() {
        let p = BrowserTabsProvider { windows: &[] };
        assert!(p.search("").await.is_empty());
    }

    #[tokio::test]
    async fn non_browser_processes_skipped_without_ffi_call() {
        // The provider should short-circuit before reaching
        // `get_browser_tabs` for non-browser processes. We can't
        // observe the FFI call directly, but we can confirm an
        // empty result for windows whose process_name doesn't
        // match the browser allowlist.
        let windows = vec![WindowEntry {
            hwnd: 1,
            pid: 0,
            process_name: "definitely-not-a-browser-12345".into(),
            title: "Not a browser".into(),
        }];
        let p = BrowserTabsProvider { windows: &windows };
        assert!(p.search("").await.is_empty());
    }

    #[test]
    fn provider_name_is_stable() {
        let p = BrowserTabsProvider { windows: &[] };
        assert_eq!(p.name(), "browser_tabs");
    }
}
