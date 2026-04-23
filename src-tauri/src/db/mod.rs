pub mod schema;

use directories::ProjectDirs;
use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open the production database in the user's platform-appropriate data dir.
    /// This is the only constructor that should be called from `run()`.
    pub fn new() -> Result<Self> {
        let path = get_db_path().expect("Could not determine database path");

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        Self::new_with_path(&path)
    }

    /// Open a database at an explicit path. Used by integration tests that
    /// need on-disk persistence but must not touch the real user data dir.
    pub fn new_with_path(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())?;
        // R-05 mitigation: WAL gives us safer crash recovery — a SIGKILL or
        // power loss mid-transaction leaves the DB file consistent (the WAL
        // journal is replayed on next open) instead of potentially corrupt
        // under the default rollback-journal mode. Also unblocks concurrent
        // reads during writes, which matters for the clipboard monitor +
        // search-on-keystroke pattern. This is a one-time PRAGMA; SQLite
        // persists the choice in the DB file header.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(schema::SCHEMA)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    /// Open an ephemeral in-memory database. Used by unit tests that need a
    /// real SQLite surface but no filesystem. Dropping this `Database` drops
    /// all state — guaranteed isolation across `cargo test` workers.
    #[cfg(any(test, feature = "test-support"))]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(schema::SCHEMA)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(sql, params)
    }

    pub fn query_map<T, F>(&self, sql: &str, params: &[&dyn rusqlite::ToSql], f: F) -> Result<Vec<T>>
    where
        F: FnMut(&rusqlite::Row<'_>) -> Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;
        rows.collect()
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
