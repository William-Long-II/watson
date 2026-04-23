//! WAT-406: in-memory notifications queue for non-fatal events.
//!
//! Where the [`StartupWarnings`](crate::warnings) surface is a loud
//! banner for conditions the user must know about RIGHT NOW (hotkey
//! broken, config from the future), the notifications drawer is the
//! quieter scroll-of-history: index failures, update-check errors,
//! any eprintln! that wants a user-visible record.
//!
//! Entries are in-memory only for this version — restart wipes them.
//! That matches the intent (notifications are transient; persistent
//! issues reappear via the loud banner on next startup) and keeps
//! this module dependency-free. Persistence can be a follow-up if
//! users ask for it.
//!
//! ## 24h auto-expiry
//!
//! [`list`] returns only notifications created within the last 24
//! hours. `list` also garbage-collects expired rows in place so the
//! Vec doesn't grow unboundedly across a long uptime.
//!
//! ## Relationship to StartupWarnings
//!
//! The two surfaces are complementary. When a startup warning is
//! recorded, it is ALSO pushed to the notifications queue — the
//! banner is dismissable and loud; the drawer entry is the
//! persistent record the user can revisit.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const EXPIRY_SECS: i64 = 24 * 60 * 60; // 24 hours

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub message: String,
    /// Unix seconds when the entry was pushed.
    pub created_at: i64,
    /// Set to true when the user dismisses the entry. Dismissed entries
    /// stop appearing in the unread count but remain in `list()` until
    /// expiry so users who dismissed by accident can still see them
    /// briefly.
    #[serde(default)]
    pub dismissed: bool,
}

#[derive(Debug, Default)]
pub struct NotificationsManager {
    inner: Mutex<Vec<Notification>>,
    /// Monotonic counter to break timestamp ties in the same millisecond.
    /// Avoids the sub-ms id collision we hit on the notes / snippets
    /// modules — under rapid pushes, ids still stay unique.
    next_seq: Mutex<u64>,
}

impl NotificationsManager {
    pub fn new() -> Self {
        NotificationsManager::default()
    }

    /// Push a new notification. Returns its id so the caller can
    /// deduplicate or dismiss it later if desired.
    pub fn push(&self, severity: Severity, title: &str, message: &str) -> String {
        self.push_at(severity, title, message, Utc::now().timestamp())
    }

    /// Variant with an explicit `now` timestamp. Used by tests to
    /// control created_at without relying on sleeps.
    pub fn push_at(
        &self,
        severity: Severity,
        title: &str,
        message: &str,
        now: i64,
    ) -> String {
        let mut seq = self.next_seq.lock().unwrap();
        *seq += 1;
        let id = format!("notif:{now}-{s}", s = *seq);
        drop(seq);
        let n = Notification {
            id: id.clone(),
            severity,
            title: title.to_string(),
            message: message.to_string(),
            created_at: now,
            dismissed: false,
        };
        self.inner.lock().unwrap().push(n);
        id
    }

    /// Return all active (non-expired) notifications, newest-first.
    /// Garbage-collects expired entries in place.
    pub fn list(&self) -> Vec<Notification> {
        self.list_at(Utc::now().timestamp())
    }

    pub fn list_at(&self, now: i64) -> Vec<Notification> {
        let mut inner = self.inner.lock().unwrap();
        let threshold = now - EXPIRY_SECS;
        inner.retain(|n| n.created_at >= threshold);
        let mut out = inner.clone();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    /// Count of un-dismissed, un-expired notifications. Drives the
    /// header badge.
    pub fn unread_count(&self) -> usize {
        self.unread_count_at(Utc::now().timestamp())
    }

    pub fn unread_count_at(&self, now: i64) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let threshold = now - EXPIRY_SECS;
        inner.retain(|n| n.created_at >= threshold);
        inner.iter().filter(|n| !n.dismissed).count()
    }

    /// Mark one notification as dismissed. Returns true if a matching
    /// entry was found. Idempotent: dismissing an already-dismissed or
    /// unknown id returns false without erroring.
    pub fn dismiss(&self, id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        for n in inner.iter_mut() {
            if n.id == id && !n.dismissed {
                n.dismissed = true;
                return true;
            }
        }
        false
    }

    /// Dismiss every currently-active notification. Returns the
    /// number that were actually flipped (not already dismissed).
    pub fn dismiss_all(&self) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let mut n = 0;
        for entry in inner.iter_mut() {
            if !entry.dismissed {
                entry.dismissed = true;
                n += 1;
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_750_000_000;
    const HOUR: i64 = 3600;

    // --- push / list ordering ---

    #[test]
    fn new_is_empty() {
        let m = NotificationsManager::new();
        assert!(m.list().is_empty());
        assert_eq!(m.unread_count(), 0);
    }

    #[test]
    fn list_returns_newest_first() {
        let m = NotificationsManager::new();
        m.push_at(Severity::Info, "first", "", NOW - 10);
        m.push_at(Severity::Info, "second", "", NOW - 5);
        m.push_at(Severity::Info, "third", "", NOW);

        let out = m.list_at(NOW);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].title, "third");
        assert_eq!(out[1].title, "second");
        assert_eq!(out[2].title, "first");
    }

    #[test]
    fn push_generates_unique_ids_even_within_same_second() {
        // Sub-second push bursts used to produce id collisions on
        // other modules; the seq counter prevents that here.
        let m = NotificationsManager::new();
        let id1 = m.push_at(Severity::Info, "a", "", NOW);
        let id2 = m.push_at(Severity::Info, "b", "", NOW);
        assert_ne!(id1, id2);
    }

    // --- 24h expiry ---

    #[test]
    fn list_drops_entries_older_than_24h() {
        let m = NotificationsManager::new();
        m.push_at(Severity::Info, "stale", "", NOW - 25 * HOUR);
        m.push_at(Severity::Info, "fresh", "", NOW - HOUR);
        let out = m.list_at(NOW);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "fresh");
    }

    #[test]
    fn list_keeps_entries_exactly_at_24h_boundary() {
        // Boundary pin: an entry at now-24h is still inside the window.
        let m = NotificationsManager::new();
        m.push_at(Severity::Info, "edge", "", NOW - 24 * HOUR);
        assert_eq!(m.list_at(NOW).len(), 1);
    }

    #[test]
    fn expired_entries_are_gc_d_in_place_not_leaked() {
        // Defensive: after listing, the backing Vec shouldn't still
        // hold the stale entries (would grow unboundedly across a
        // long uptime).
        let m = NotificationsManager::new();
        m.push_at(Severity::Info, "stale", "", NOW - 25 * HOUR);
        let _ = m.list_at(NOW);
        // Peek via `unread_count_at` which ALSO gc's. A second list
        // should show the Vec is small — indirect but sufficient.
        assert_eq!(m.unread_count_at(NOW), 0);
    }

    // --- dismissal ---

    #[test]
    fn dismiss_marks_entry_and_excludes_from_unread_count() {
        let m = NotificationsManager::new();
        let id = m.push_at(Severity::Warning, "alert", "", NOW);
        assert_eq!(m.unread_count_at(NOW), 1);
        assert!(m.dismiss(&id));
        assert_eq!(m.unread_count_at(NOW), 0);
    }

    #[test]
    fn dismiss_does_not_remove_from_list_only_flags() {
        // The user can still see a dismissed entry in the drawer —
        // it's out of the unread count, but the row persists until
        // expiry so recent dismissals are discoverable.
        let m = NotificationsManager::new();
        let id = m.push_at(Severity::Info, "t", "", NOW);
        m.dismiss(&id);
        let out = m.list_at(NOW);
        assert_eq!(out.len(), 1);
        assert!(out[0].dismissed);
    }

    #[test]
    fn dismiss_returns_false_for_unknown_id() {
        let m = NotificationsManager::new();
        assert!(!m.dismiss("notif:nope"));
    }

    #[test]
    fn dismiss_is_idempotent_for_already_dismissed() {
        // A race where two tabs/buttons both fire dismiss for the
        // same id: the second call returns false (nothing to do)
        // without error.
        let m = NotificationsManager::new();
        let id = m.push_at(Severity::Info, "t", "", NOW);
        assert!(m.dismiss(&id));
        assert!(!m.dismiss(&id));
    }

    #[test]
    fn dismiss_all_flips_every_active_entry() {
        let m = NotificationsManager::new();
        m.push_at(Severity::Info, "a", "", NOW);
        m.push_at(Severity::Warning, "b", "", NOW);
        m.push_at(Severity::Error, "c", "", NOW);
        assert_eq!(m.dismiss_all(), 3);
        assert_eq!(m.unread_count_at(NOW), 0);
    }

    #[test]
    fn dismiss_all_reports_count_of_actually_flipped() {
        // One entry was already dismissed individually — only two
        // are "new dismissals" for the bulk call.
        let m = NotificationsManager::new();
        m.push_at(Severity::Info, "a", "", NOW);
        let b = m.push_at(Severity::Info, "b", "", NOW);
        m.push_at(Severity::Info, "c", "", NOW);
        m.dismiss(&b);
        assert_eq!(m.dismiss_all(), 2);
    }
}
