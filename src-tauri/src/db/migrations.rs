//! Schema migration framework for the Watson SQLite database.
//!
//! Closes R-02: before this module, schema changes were applied via
//! `CREATE TABLE IF NOT EXISTS` on every startup. New columns, indexes,
//! or table renames could not be rolled out safely because there was no
//! notion of "what version is this user's DB at."
//!
//! The `rusqlite_migration` crate tracks applied migrations via
//! `PRAGMA user_version`. Fresh DBs start at 0; after running migration
//! N, `user_version = N`. The framework refuses to go backward.
//!
//! ## Adding a migration
//!
//! Append to the `Vec` in `migrations()`. Never edit or reorder existing
//! migrations — users in the wild have already applied them and the
//! framework will crash if the content of a previously-applied migration
//! changes. Prefer `CREATE TABLE IF NOT EXISTS` / `ADD COLUMN` / etc. so
//! the migration is idempotent should it ever be re-run.

use rusqlite_migration::{M, Migrations};

/// All migrations in application order. Length of this Vec is the schema
/// version after a fresh install.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        // 001 — initial schema. Baseline for existing users (pre-migration
        // DBs have all these tables already; IF NOT EXISTS makes the run a
        // no-op). Fresh installs get the full schema on first migrate.
        M::up(SCHEMA_V001),
    ])
}

const SCHEMA_V001: &str = r#"
CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    icon_cache_path TEXT,
    launch_count INTEGER DEFAULT 0,
    last_launched INTEGER,
    platform TEXT NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS search_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query TEXT NOT NULL,
    selected_item_id TEXT,
    selected_item_type TEXT,
    timestamp INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS icons (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    cache_path TEXT NOT NULL,
    hash TEXT NOT NULL,
    cached_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS note_tags (
    note_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (note_id, tag),
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    extension TEXT,
    size_bytes INTEGER,
    modified_at INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS scratchpad (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    content TEXT NOT NULL DEFAULT '',
    modified_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_apps_name ON apps(name);
CREATE INDEX IF NOT EXISTS idx_history_query ON search_history(query);
CREATE INDEX IF NOT EXISTS idx_history_timestamp ON search_history(timestamp);
CREATE INDEX IF NOT EXISTS idx_notes_title ON notes(title);
CREATE INDEX IF NOT EXISTS idx_notes_modified ON notes(modified_at);
CREATE INDEX IF NOT EXISTS idx_note_tags_tag ON note_tags(tag);
CREATE INDEX IF NOT EXISTS idx_files_name ON files(name);
CREATE INDEX IF NOT EXISTS idx_files_extension ON files(extension);
CREATE INDEX IF NOT EXISTS idx_files_modified ON files(modified_at);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrations_validate() {
        // rusqlite_migration's validate() checks that every M::up has a
        // parseable, executable SQL body. If someone adds a malformed
        // migration, this fires before it reaches users.
        migrations().validate().expect("migrations should validate");
    }

    #[test]
    fn fresh_db_migrates_to_latest_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut conn).expect("migrate");

        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        // After the single current migration, user_version should be 1.
        // When new migrations are added, update this expectation.
        assert_eq!(version, 1);
    }

    #[test]
    fn migrating_already_current_db_is_noop() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut conn).expect("first migrate");
        // Second call must succeed and leave the version unchanged.
        migrations().to_latest(&mut conn).expect("idempotent re-migrate");
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn legacy_db_with_tables_but_no_version_migrates_cleanly() {
        // Simulates an existing user's database created pre-migration
        // framework: tables already exist, user_version is still 0. The
        // migration must succeed (CREATE TABLE IF NOT EXISTS is idempotent)
        // and bump user_version to 1.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V001).unwrap();
        // Still at 0 because we applied schema directly, not via migrations.
        let pre: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pre, 0);

        migrations().to_latest(&mut conn).expect("migrate");

        let post: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(post, 1);
    }

    #[test]
    fn migrations_produce_all_expected_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut conn).expect("migrate");
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
                .unwrap();
            assert_eq!(exists, 1, "expected table {table}");
        }
    }

    #[test]
    fn preexisting_data_survives_migration() {
        // If a user has notes in their pre-migration DB, running the
        // migration framework for the first time must not drop them.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_V001).unwrap();
        conn.execute(
            "INSERT INTO notes (id, title, content, created_at, modified_at) VALUES (?, ?, ?, ?, ?)",
            ("note:test", "t", "body", 100, 100),
        )
        .unwrap();

        migrations().to_latest(&mut conn).expect("migrate");

        let title: String = conn
            .query_row("SELECT title FROM notes WHERE id = ?", ["note:test"], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "t");
    }
}
