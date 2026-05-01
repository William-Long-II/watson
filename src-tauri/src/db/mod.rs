pub mod migrations;

use directories::ProjectDirs;
use rusqlite::Connection;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Unified error for database operations. Migrations can fail with
/// `rusqlite_migration::Error`, connection open/pragma operations with
/// `rusqlite::Error`; boxing both lets the constructors stay on a single
/// signature without introducing a custom enum.
pub type DbResult<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open the production database in the user's platform-appropriate data dir.
    /// This is the only constructor that should be called from `run()`.
    pub fn new() -> DbResult<Self> {
        let path = get_db_path().ok_or("could not determine database path")?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        Self::new_with_path(&path)
    }

    /// Open a database at an explicit path. Used by integration tests that
    /// need on-disk persistence but must not touch the real user data dir.
    pub fn new_with_path(path: impl AsRef<Path>) -> DbResult<Self> {
        let mut conn = Connection::open(path.as_ref())?;
        // R-05 mitigation: WAL gives us safer crash recovery — a SIGKILL or
        // power loss mid-transaction leaves the DB file consistent (the WAL
        // journal is replayed on next open) instead of potentially corrupt
        // under the default rollback-journal mode. Also unblocks concurrent
        // reads during writes, which matters for the clipboard monitor +
        // search-on-keystroke pattern. This is a one-time PRAGMA; SQLite
        // persists the choice in the DB file header.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // R-02 mitigation: run the migration framework. Idempotent — legacy
        // DBs that already have the tables (from the pre-migration
        // execute_batch path) see the v1 baseline applied as a no-op and
        // have user_version bumped to 1.
        migrations::migrations().to_latest(&mut conn)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    /// Open an ephemeral in-memory database. Used by unit tests that need a
    /// real SQLite surface but no filesystem. Dropping this `Database` drops
    /// all state — guaranteed isolation across `cargo test` workers.
    #[cfg(any(test, feature = "test-support"))]
    pub fn in_memory() -> DbResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrations::migrations().to_latest(&mut conn)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(sql, params)
    }

    pub fn query_map<T, F>(
        &self,
        sql: &str,
        params: &[&dyn rusqlite::ToSql],
        f: F,
    ) -> rusqlite::Result<Vec<T>>
    where
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;
        rows.collect()
    }

    /// Run a closure within a database transaction. If the closure returns
    /// `Err`, the transaction is rolled back. If it returns `Ok`, the
    /// transaction is committed.
    ///
    /// The closure receives a reference to the `rusqlite::Connection`.
    pub fn with_transaction<T, F>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&rusqlite::Connection) -> DbResult<T>,
    {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        match f(&tx) {
            Ok(result) => {
                tx.commit()?;
                Ok(result)
            }
            Err(e) => {
                // tx.rollback() is called automatically on drop if not committed
                Err(e)
            }
        }
    }
}

fn get_db_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.data_dir().join("watson.db"))
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub icon_cache_path: Option<String>,
    pub launch_count: i32,
    pub last_launched: Option<i64>,
    pub platform: String,
    /// Gemini Improvement #5: The modification time of the executable or
    /// app bundle, used for icon cache invalidation.
    pub modified_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_with_path_enables_wal_journal_mode() {
        // R-05 mitigation: PRAGMA journal_mode=WAL must be active on every
        // disk-backed Database so a crash mid-write leaves the file
        // consistent. In-memory databases don't support WAL — we only
        // assert this for the on-disk constructor.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new_with_path(&path).expect("open");
        let mode: String = db
            .conn
            .lock()
            .unwrap()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("query journal_mode");
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn new_with_path_creates_expected_tables() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new_with_path(&path).expect("open");
        let conn = db.conn.lock().unwrap();
        // Query sqlite_master for each table we rely on. If one is missing,
        // production queries against it will fail at runtime — catch it here.
        for table in [
            "apps",
            "search_history",
            "icons",
            "notes",
            "note_tags",
            "files",
            "scratchpad",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |row| row.get(0),
                )
                .expect("sqlite_master query");
            assert_eq!(exists, 1, "expected table {table} to exist");
        }
    }
}
