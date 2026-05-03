//! Handler for `SearchAction::FocusWindow`.
//!
//! Cross-platform window focus delegated to the existing
//! `actions::windows::focus_window` (Win32 SetForegroundWindow +
//! AttachThreadInput on Windows; AX `kAXRaiseAction` +
//! `NSRunningApplication.activate` on macOS; EWMH
//! `_NET_ACTIVE_WINDOW` ClientMessage on X11; wlr-foreign-toplevel
//! `activate` on wlroots Wayland).

use crate::actions::windows::focus_window as os_focus_window;

pub fn handle(hwnd: i64) -> Result<(), String> {
    os_focus_window(hwnd)
}
