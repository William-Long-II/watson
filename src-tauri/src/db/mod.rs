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
