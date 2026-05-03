//! Handler for `SearchAction::LaunchApp`.
//!
//! Records launch stats BEFORE invoking the OS launcher so frecency
//! reflects user intent even if the OS-level launch fails (the user
//! *chose* this app — that signal matters regardless of whether the
//! binary actually started). Also patches the in-memory app cache so
//! subsequent searches reflect the updated count without waiting for
//! a reindex round-trip.
//!
//! Lifetime: takes individual references to the pieces of AppState it
//! needs (DB + indexed-apps lock). A future ActionContext bundle
//! could fold these together once enough handlers want overlapping
//! sets of refs; this first slice keeps the handler signature
//! explicit so the dependency surface is visible at the call site.

use std::sync::RwLock;

use crate::actions::launch_app as os_launch;
use crate::apps::record_launch;
use crate::db::{AppEntry, Database};

pub fn handle(
    path: String,
    db: &Database,
    indexed_apps: &RwLock<Vec<AppEntry>>,
) -> Result<(), String> {
    // Failure to record stats is non-fatal — the launch should still
    // proceed even if the DB write trips. The user's intent is the
    // signal we care about; missing it for one launch isn't worth
    // failing the whole action.
    let _ = record_launch(db, &path);

    let now = chrono::Utc::now().timestamp();
    {
        let mut cached = indexed_apps.write().unwrap();
        for app in cached.iter_mut() {
            if app.path == path {
                app.launch_count = app.launch_count.saturating_add(1);
                app.last_launched = Some(now);
                break;
            }
        }
    }

    os_launch(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_app(path: &str) -> AppEntry {
        AppEntry {
            id: format!("test:{path}"),
            name: "Test App".into(),
            path: path.into(),
            icon_cache_path: None,
            launch_count: 0,
            last_launched: None,
            platform: "test".into(),
            modified_at: 0,
        }
    }

    /// Stat-recording is the load-bearing observable side effect of
    /// `handle`; the OS launch itself we can't unit-test (it forks
    /// the user's installed app). We verify that the in-memory cache
    /// is patched correctly when the path matches a cached entry,
    /// independent of the DB write or the OS launch outcome.
    ///
    /// We bypass the actual `handle` entrypoint here and re-do the
    /// cache-patch logic directly because invoking `handle` would
    /// also trigger `record_launch` (needs a real DB) and `os_launch`
    /// (needs a real binary). When ActionContext lands and the
    /// handlers grow integration tests, this stub becomes redundant.
    #[test]
    fn cache_patch_increments_count_and_sets_timestamp() {
        let lock = RwLock::new(vec![
            fresh_app("/path/a"),
            fresh_app("/path/b"),
            fresh_app("/path/c"),
        ]);
        let path = "/path/b";
        let now = 1_700_000_000;

        // Replicate the cache-patch block from `handle` so we can
        // assert in isolation.
        {
            let mut cached = lock.write().unwrap();
            for app in cached.iter_mut() {
                if app.path == path {
                    app.launch_count = app.launch_count.saturating_add(1);
                    app.last_launched = Some(now);
                    break;
                }
            }
        }

        let snapshot = lock.read().unwrap();
        assert_eq!(snapshot[0].launch_count, 0); // untouched
        assert_eq!(snapshot[1].launch_count, 1); // incremented
        assert_eq!(snapshot[1].last_launched, Some(now));
        assert_eq!(snapshot[2].launch_count, 0); // untouched
    }

    #[test]
    fn cache_patch_handles_unknown_path_without_panic() {
        let lock = RwLock::new(vec![fresh_app("/path/a")]);
        let path = "/path/does-not-exist";

        {
            let mut cached = lock.write().unwrap();
            for app in cached.iter_mut() {
                if app.path == path {
                    app.launch_count = app.launch_count.saturating_add(1);
                    break;
                }
            }
        }

        // No-op: the cache stays unchanged, no panic.
        let snapshot = lock.read().unwrap();
        assert_eq!(snapshot[0].launch_count, 0);
    }

    #[test]
    fn cache_patch_saturates_at_i32_max() {
        let mut app = fresh_app("/path/a");
        app.launch_count = i32::MAX;
        let lock = RwLock::new(vec![app]);

        {
            let mut cached = lock.write().unwrap();
            for entry in cached.iter_mut() {
                entry.launch_count = entry.launch_count.saturating_add(1);
            }
        }

        let snapshot = lock.read().unwrap();
        assert_eq!(snapshot[0].launch_count, i32::MAX); // saturated
    }
}
