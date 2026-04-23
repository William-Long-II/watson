//! WAT-105 / R-11: startup-time, non-fatal warnings surfaced to the UI.
//!
//! Before this module existed, a global-shortcut registration failure
//! either panicked the Tauri setup closure or was silently dropped — the
//! user had no way to know their hotkey never registered and the app
//! appeared unresponsive. Now: failures land here, the app still starts,
//! and the frontend polls `get_startup_warnings` to render a banner with a
//! "Change hotkey" escape hatch.
//!
//! Keep warnings lightweight. Anything larger (index errors, update
//! failures) should move into the WAT-406 notifications drawer; the
//! startup-warnings list is specifically for conditions the app couldn't
//! recover from during `setup()`.

use serde::{Deserialize, Serialize};
use std::sync::RwLock;

/// Categories of startup warnings. Kept as a named enum so the frontend
/// can switch on it for an appropriate call-to-action button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupWarningKind {
    /// Global hotkey registration failed — usually because the combo is
    /// already taken by another app (common on some Linux WMs, macOS
    /// system preferences, Windows apps that bind Alt+Space).
    ShortcutUnavailable,
    /// WAT-206: config.toml was written by a Watson newer than this binary.
    /// We're running with defaults; the user's real config is untouched
    /// on disk and they'll get it back when they upgrade again.
    SettingsFromNewerVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupWarning {
    /// Stable id so the UI can dismiss a specific warning without
    /// accidentally clearing a newer one with the same message.
    pub id: String,
    pub kind: StartupWarningKind,
    /// Human-readable message rendered in the banner verbatim.
    pub message: String,
}

#[derive(Debug, Default)]
pub struct StartupWarnings {
    inner: RwLock<Vec<StartupWarning>>,
}

impl StartupWarnings {
    pub fn new() -> Self {
        StartupWarnings::default()
    }

    pub fn push(&self, warning: StartupWarning) {
        self.inner.write().unwrap().push(warning);
    }

    /// Snapshot of all active warnings, cloned so the caller doesn't hold
    /// the lock across an await boundary (Tauri commands can be async).
    pub fn list(&self) -> Vec<StartupWarning> {
        self.inner.read().unwrap().clone()
    }

    /// Remove a warning by id. Returns true if something was removed.
    /// Dismissing an unknown id is not an error — the UI may race the
    /// backend and ask to dismiss a warning that another caller already
    /// cleared.
    pub fn dismiss(&self, id: &str) -> bool {
        let mut inner = self.inner.write().unwrap();
        let before = inner.len();
        inner.retain(|w| w.id != id);
        inner.len() != before
    }

    /// WAT-206: record that the on-disk config was written by a newer
    /// Watson. Uses a fixed id so the banner deduplicates if somehow the
    /// helper is invoked more than once per startup.
    pub fn record_settings_from_newer_version(
        &self,
        file_version: u32,
        supported_version: u32,
    ) {
        let id = "settings-from-newer-version".to_string();
        let _ = self.dismiss(&id);
        self.push(StartupWarning {
            id,
            kind: StartupWarningKind::SettingsFromNewerVersion,
            message: format!(
                "Your config.toml was written by a newer Watson (schema v{file_version}); \
                 this version supports up to v{supported_version}. Running with defaults. \
                 Your config is preserved on disk — upgrade Watson to use it again."
            ),
        });
    }

    /// Convenience constructor for the common case: the global hotkey
    /// couldn't be bound. Uses a deterministic id so repeated calls for
    /// the same hotkey replace rather than stack.
    pub fn record_shortcut_unavailable(&self, hotkey: &str, reason: &str) {
        let id = format!("shortcut-unavailable:{hotkey}");
        // Deduplicate: if we already recorded this hotkey, drop the old
        // entry so the newest reason wins. `dismiss` returns false on
        // first call; we don't care about its result here.
        let _ = self.dismiss(&id);
        self.push(StartupWarning {
            id,
            kind: StartupWarningKind::ShortcutUnavailable,
            message: format!(
                "Couldn't register hotkey {hotkey}: {reason}. Another app may be using it. \
                 Pick a different hotkey in Settings."
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let w = StartupWarnings::new();
        assert!(w.list().is_empty());
    }

    #[test]
    fn push_is_visible_to_list() {
        let w = StartupWarnings::new();
        w.push(StartupWarning {
            id: "x".into(),
            kind: StartupWarningKind::ShortcutUnavailable,
            message: "example".into(),
        });
        let list = w.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "x");
    }

    #[test]
    fn dismiss_by_id_removes_exactly_that_entry() {
        let w = StartupWarnings::new();
        w.push(StartupWarning {
            id: "a".into(),
            kind: StartupWarningKind::ShortcutUnavailable,
            message: "".into(),
        });
        w.push(StartupWarning {
            id: "b".into(),
            kind: StartupWarningKind::ShortcutUnavailable,
            message: "".into(),
        });

        assert!(w.dismiss("a"));

        let remaining: Vec<String> = w.list().into_iter().map(|x| x.id).collect();
        assert_eq!(remaining, vec!["b".to_string()]);
    }

    #[test]
    fn dismiss_unknown_id_returns_false_and_does_nothing() {
        let w = StartupWarnings::new();
        w.push(StartupWarning {
            id: "a".into(),
            kind: StartupWarningKind::ShortcutUnavailable,
            message: "".into(),
        });
        assert!(!w.dismiss("nope"));
        assert_eq!(w.list().len(), 1);
    }

    #[test]
    fn record_shortcut_unavailable_produces_a_deterministic_id() {
        // Contract: the id is derived from the hotkey string alone. This
        // matters for the de-dup behavior below and for the frontend's
        // "dismissed this once, don't resurface" logic (when that lands).
        let w = StartupWarnings::new();
        w.record_shortcut_unavailable("Alt+Space", "already bound");
        let id = w.list()[0].id.clone();
        assert_eq!(id, "shortcut-unavailable:Alt+Space");
    }

    #[test]
    fn record_shortcut_unavailable_dedupes_on_same_hotkey() {
        // Calling twice with the same hotkey must not stack two banners —
        // Tauri's setup may retry registration depending on plugin impl.
        let w = StartupWarnings::new();
        w.record_shortcut_unavailable("Alt+Space", "reason 1");
        w.record_shortcut_unavailable("Alt+Space", "reason 2");
        assert_eq!(w.list().len(), 1);
        // The newest reason wins.
        assert!(w.list()[0].message.contains("reason 2"));
    }

    #[test]
    fn record_shortcut_unavailable_keeps_different_hotkeys_separate() {
        let w = StartupWarnings::new();
        w.record_shortcut_unavailable("Alt+Space", "");
        w.record_shortcut_unavailable("Ctrl+Shift+P", "");
        assert_eq!(w.list().len(), 2);
    }

    #[test]
    fn dismiss_clears_a_shortcut_warning() {
        let w = StartupWarnings::new();
        w.record_shortcut_unavailable("Alt+Space", "");
        assert!(w.dismiss("shortcut-unavailable:Alt+Space"));
        assert!(w.list().is_empty());
    }

    // --- WAT-206: settings schema-version warning ---

    #[test]
    fn record_settings_from_newer_version_mentions_both_versions() {
        let w = StartupWarnings::new();
        w.record_settings_from_newer_version(2, 1);
        let warning = &w.list()[0];
        assert_eq!(warning.kind, StartupWarningKind::SettingsFromNewerVersion);
        assert!(warning.message.contains("v2"));
        assert!(warning.message.contains("v1"));
    }

    #[test]
    fn record_settings_from_newer_version_is_idempotent() {
        // Shouldn't stack duplicate banners if invoked twice per startup.
        let w = StartupWarnings::new();
        w.record_settings_from_newer_version(2, 1);
        w.record_settings_from_newer_version(2, 1);
        assert_eq!(w.list().len(), 1);
    }
}
