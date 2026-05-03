//! Handler for `SearchAction::FocusBrowserTab`.
//!
//! Cross-platform browser-tab activation delegated to the existing
//! `actions::browser_tabs::focus_browser_tab` (UIA on Windows, AX
//! tree walk on macOS, AT-SPI on Linux). The handler exists for
//! symmetry with the rest of the action surface; if tab focus
//! grows pre-flight checks or telemetry, that lands here.

use crate::actions::browser_tabs::focus_browser_tab as os_focus_browser_tab;

pub fn handle(hwnd: i64, index: i32) -> Result<(), String> {
    os_focus_browser_tab(hwnd, index)
}
