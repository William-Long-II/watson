//! App launch-stat persistence for WAT-201 usage-weighted ranking.
//!
//! The indexer rediscovers apps from the filesystem each run and resets
//! their in-memory `launch_count` / `last_launched` to 0 / None. This
//! module stores the stats in a separate `app_launches` table keyed by
//! path — decoupled from the app inventory so stats survive reindex and
//! we don't have to persist every discovered app row.
//!
//! - [`record_launch`] upserts a row with incremented `launch_count` and
//!   `last_launched = now`. Called from the `LaunchApp` branch of
//!   `execute_action`.
//! - [`merge_stats_into_apps`] reads the table in one round-trip and
//!   patches the in-memory `Vec<AppEntry>` cache with the persisted
//!   stats. Called on startup and after every `reindex_apps`.
//!
//! ## Keying
//!
//! Stats are keyed by path. The indexer generates ids that depend on
//! filesystem layout; a user moving their app bundle would break a
//! pure-id key. Path is stable enough for the app lifetime. When an app
//! IS relocated, its stats reset — treated as acceptable (a follow-up
//! ticket could key by bundle id on macOS and exe name on Windows if
//! this turns out to matter).

use crate::db::{AppEntry, Database};
use chrono::Utc;
use std::collections::HashMap;

/// Record a launch: upsert `app_launches` for `path` with count +1 and
/// `last_launched = now()`. A failed DB write is swallowed at the caller
/// boundary — the launch itself must not be blocked by a stats failure.
pub fn record_launch(db: &Database, path: &str) -> Result<(), String> {
    db.execute(
        "INSERT INTO app_launches (path, launch_count, last_launched) VALUES (?, 1, ?)
         ON CONFLICT(path) DO UPDATE SET
            launch_count = launch_count + 1,
            last_launched = excluded.last_launched",
        &[&path, &Utc::now().timestamp()],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Load every row's `(path, launch_count, last_launched)` tuple and patch
/// it onto the matching `AppEntry` in `apps`. Apps with no row keep their
/// indexer-provided defaults (launch_count=0, last_launched=None).
pub fn merge_stats_into_apps(db: &Database, apps: &mut [AppEntry]) -> Result<(), String> {
    let rows: Vec<(String, i32, Option<i64>)> = db
        .query_map(
            "SELECT path, launch_count, last_launched FROM app_launches",
            &[],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    // HashMap lookup keeps the merge linear in apps.len() rather than
    // apps.len() * db_row_count.
    let by_path: HashMap<String, (i32, Option<i64>)> = rows
        .into_iter()
        .map(|(p, c, l)| (p, (c, l)))
        .collect();

    for app in apps.iter_mut() {
        if let Some(&(count, last)) = by_path.get(&app.path) {
            app.launch_count = count;
            app.last_launched = last;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::Arc;

    fn db() -> Arc<Database> {
        Arc::new(Database::in_memory().expect("in-memory DB"))
    }

    fn entry(path: &str) -> AppEntry {
        AppEntry {
            id: format!("app:{path}"),
            name: format!("App at {path}"),
            path: path.to_string(),
            icon_cache_path: None,
            launch_count: 0,
            last_launched: None,
            platform: "test".to_string(),
        }
    }

    /// Seed a row directly in the app_launches table. Separate from
    /// record_launch so the tests can set up controlled `last_launched`
    /// timestamps and verify record_launch's behavior against them.
    fn seed_launch_row(db: &Database, path: &str, launch_count: i32, last_launched: Option<i64>) {
        db.execute(
            "INSERT INTO app_launches (path, launch_count, last_launched) VALUES (?, ?, ?)",
            &[&path, &launch_count, &last_launched],
        )
        .expect("seed insert");
    }

    // --- record_launch ---

    #[test]
    fn record_launch_creates_row_for_first_time_launch() {
        // New-path case: record_launch must upsert, not rely on a
        // pre-existing row. This is how the prod flow works — indexer
        // doesn't touch app_launches, only LaunchApp actions do.
        let db = db();
        record_launch(&db, "/usr/bin/chrome").unwrap();
        let count: i32 = db
            .query_map(
                "SELECT launch_count FROM app_launches WHERE path = ?",
                &[&"/usr/bin/chrome"],
                |r| r.get(0),
            )
            .unwrap()[0];
        assert_eq!(count, 1);
    }

    #[test]
    fn record_launch_increments_count_on_existing_row() {
        let db = db();
        seed_launch_row(&db, "/usr/bin/chrome", 3, None);
        record_launch(&db, "/usr/bin/chrome").unwrap();

        let count: i32 = db
            .query_map(
                "SELECT launch_count FROM app_launches WHERE path = ?",
                &[&"/usr/bin/chrome"],
                |r| r.get(0),
            )
            .unwrap()[0];
        assert_eq!(count, 4);
    }

    #[test]
    fn record_launch_sets_last_launched_to_roughly_now() {
        let db = db();
        let before = Utc::now().timestamp();
        record_launch(&db, "/usr/bin/chrome").unwrap();
        let after = Utc::now().timestamp();

        let last: i64 = db
            .query_map(
                "SELECT last_launched FROM app_launches WHERE path = ?",
                &[&"/usr/bin/chrome"],
                |r| r.get(0),
            )
            .unwrap()[0];
        assert!(last >= before && last <= after, "last_launched out of range: {last}");
    }

    // --- merge_stats_into_apps ---

    #[test]
    fn merge_patches_stats_onto_matching_path() {
        let db = db();
        seed_launch_row(&db, "/chrome", 7, Some(1_700_000_000));
        let mut apps = vec![entry("/chrome"), entry("/firefox")];

        merge_stats_into_apps(&db, &mut apps).unwrap();

        assert_eq!(apps[0].launch_count, 7);
        assert_eq!(apps[0].last_launched, Some(1_700_000_000));
        // Unseeded app keeps defaults.
        assert_eq!(apps[1].launch_count, 0);
        assert_eq!(apps[1].last_launched, None);
    }

    #[test]
    fn merge_is_empty_db_safe() {
        let db = db();
        let mut apps = vec![entry("/chrome")];
        merge_stats_into_apps(&db, &mut apps).unwrap();
        assert_eq!(apps[0].launch_count, 0);
    }

    #[test]
    fn merge_does_not_duplicate_apps() {
        // Defensive: merge must not append rows, only patch in place.
        let db = db();
        seed_launch_row(&db, "/chrome", 1, None);
        let mut apps = vec![entry("/chrome")];
        merge_stats_into_apps(&db, &mut apps).unwrap();
        assert_eq!(apps.len(), 1);
    }

    #[test]
    fn record_launch_then_merge_round_trips() {
        // End-to-end: LaunchApp writes, next search reads.
        let db = db();
        record_launch(&db, "/chrome").unwrap();

        let mut apps = vec![entry("/chrome")];
        merge_stats_into_apps(&db, &mut apps).unwrap();
        assert_eq!(apps[0].launch_count, 1);
        assert!(apps[0].last_launched.is_some());
    }
}
