pub mod storage;
pub mod tags;

use crate::db::{Database, DbResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use rusqlite::ToSql;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub modified_at: i64,
    /// WAT-204: set only when `get()` detected that the on-disk .md file
    /// was modified outside Watson (vim, Obsidian, etc.) since the DB
    /// was last updated. Carries the disk content so the UI can show
    /// the reconcile dialog without a second round-trip. `None` when
    /// the note is in sync (the common case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_changes: Option<ExternalChanges>,
}

/// Snapshot of a note's on-disk state when it has diverged from the DB.
/// Populated only by `get()` on detection; callers use this to present a
/// reconcile dialog without re-reading the file themselves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalChanges {
    pub disk_title: String,
    pub disk_content: String,
    pub disk_modified_at: i64,
}

/// Tolerance for clock/filesystem drift between `modified_at` (DB, set by
/// Watson during `update`) and the file's mtime (set by the OS when Watson
/// or an external editor writes the file). Watson's own write sets both
/// within milliseconds, so anything beyond this threshold is treated as an
/// external edit. 2 seconds covers typical FAT mtime granularity and
/// reasonable NTP skew.
const EXTERNAL_EDIT_DRIFT_TOLERANCE_SECS: i64 = 2;

pub struct NotesManager {
    db: Arc<Database>,
    storage_path: std::path::PathBuf,
}

impl NotesManager {
    pub fn new(db: Arc<Database>, storage_path: std::path::PathBuf) -> Self {
        // Ensure storage directory exists
        std::fs::create_dir_all(&storage_path).ok();
        NotesManager { db, storage_path }
    }

    pub async fn create(&self, title: &str, content: &str) -> Result<Note, String> {
        let id = format!("note:{}", Utc::now().timestamp_millis());
        let now = Utc::now().timestamp();
        let extracted_tags = tags::extract_tags(content);

        // Move DB transaction to a blocking task if it's intensive, but here
        // the bottleneck is file I/O, which is now async.
        // Rust's with_transaction still blocks the current thread, so we'll
        // wrap the whole transactional block in spawn_blocking to be safe,
        // or just accept the tiny DB block and focus on the async file I/O.
        // Actually, with_transaction takes a closure. We can't await inside it.
        // We'll perform DB staged changes, then file write, then commit.
        
        let db = self.db.clone();
        let storage_path = self.storage_path.clone();
        let id_clone = id.clone();
        let title_clone = title.to_string();
        let content_clone = content.to_string();
        let tags_clone = extracted_tags.clone();

        // Transaction orchestration:
        // 1. Start DB transaction
        // 2. Perform DB ops
        // 3. Perform Async File I/O
        // 4. Commit DB transaction
        
        // Since we can't easily await inside the rusqlite transaction closure,
        // we'll use a manual transaction approach if needed, or just use 
        // spawn_blocking for the whole synchronous-feeling block.
        // The goal of Improvement #1 is "Background File I/O". 
        // spawn_blocking is perfect for this.

        tokio::task::spawn_blocking(move || {
            db.with_transaction(|tx| {
                // Insert into database
                tx.execute(
                    "INSERT INTO notes (id, title, content, created_at, modified_at) VALUES (?, ?, ?, ?, ?)",
                    &[&id_clone as &dyn ToSql, &title_clone, &content_clone, &now, &now],
                ).map_err(|e| e.to_string())?;

                // Insert tags
                for tag in &tags_clone {
                    tx.execute(
                        "INSERT OR IGNORE INTO note_tags (note_id, tag) VALUES (?, ?)",
                        &[&id_clone as &dyn ToSql, tag as &dyn ToSql],
                    ).map_err(|e| e.to_string())?;
                }

                // Write to file - we use a block_on here because we are already 
                // in spawn_blocking and want to keep the transactional integrity.
                // This effectively makes the file I/O happen on a background thread.
                let rt = tokio::runtime::Handle::current();
                rt.block_on(storage::write_note_file(&storage_path, &id_clone, &title_clone, &content_clone))?;

                Ok(())
            })
        }).await.map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        Ok(Note {
            id,
            title: title.to_string(),
            content: content.to_string(),
            tags: extracted_tags,
            created_at: now,
            modified_at: now,
            external_changes: None,
        })
    }

    pub async fn update(&self, id: &str, title: &str, content: &str) -> Result<Note, String> {
        let now = Utc::now().timestamp();
        let extracted_tags = tags::extract_tags(content);

        let db = self.db.clone();
        let storage_path = self.storage_path.clone();
        let id_clone = id.to_string();
        let title_clone = title.to_string();
        let content_clone = content.to_string();
        let tags_clone = extracted_tags.clone();

        let created_at = tokio::task::spawn_blocking(move || {
            db.with_transaction(|tx| {
                // Get created_at before updating
                let created_at: i64 = tx
                    .query_row(
                        "SELECT created_at FROM notes WHERE id = ?",
                        &[&id_clone as &dyn ToSql],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;

                // Update database
                tx.execute(
                    "UPDATE notes SET title = ?, content = ?, modified_at = ? WHERE id = ?",
                    &[&title_clone as &dyn ToSql, &content_clone as &dyn ToSql, &now as &dyn ToSql, &id_clone as &dyn ToSql],
                ).map_err(|e| e.to_string())?;

                // Update tags
                tx.execute("DELETE FROM note_tags WHERE note_id = ?", &[&id_clone as &dyn ToSql])
                    .map_err(|e| e.to_string())?;
                for tag in &tags_clone {
                    tx.execute(
                        "INSERT OR IGNORE INTO note_tags (note_id, tag) VALUES (?, ?)",
                        &[&id_clone as &dyn ToSql, tag as &dyn ToSql],
                    ).map_err(|e| e.to_string())?;
                }

                // Update file
                let rt = tokio::runtime::Handle::current();
                rt.block_on(storage::write_note_file(&storage_path, &id_clone, &title_clone, &content_clone))?;

                Ok(created_at)
            })
        }).await.map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        Ok(Note {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            tags: extracted_tags,
            created_at,
            modified_at: now,
            external_changes: None,
        })
    }

    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let id_clone = id.to_string();
        let db = self.db.clone();
        let storage_path = self.storage_path.clone();

        tokio::task::spawn_blocking(move || {
            db.execute("DELETE FROM notes WHERE id = ?", &[&id_clone])
                .map_err(|e| e.to_string())?;
            
            let rt = tokio::runtime::Handle::current();
            rt.block_on(storage::delete_note_file(&storage_path, &id_clone))?;
            Ok(())
        }).await.map_err(|e| e.to_string())?
    }

    pub async fn get(&self, id: &str) -> Result<Option<Note>, String> {
        let id_clone = id.to_string();
        let db = self.db.clone();

        let notes = tokio::task::spawn_blocking(move || {
            db.query_map(
                "SELECT id, title, content, created_at, modified_at FROM notes WHERE id = ?",
                &[&id_clone],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
        }).await.map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        if let Some((id, title, content, created_at, modified_at)) = notes.into_iter().next() {
            let note_tags = self.get_tags(&id).await?;
            let external_changes = self.detect_external_changes(&id, modified_at).await;
            Ok(Some(Note {
                id,
                title,
                content,
                tags: note_tags,
                created_at,
                modified_at,
                external_changes,
            }))
        } else {
            Ok(None)
        }
    }

    async fn detect_external_changes(&self, id: &str, db_modified_at: i64) -> Option<ExternalChanges> {
        let file_path = storage::find_note_file(&self.storage_path, id).await?;
        let disk_mtime = storage::file_modified_at(&file_path).await?;
        if disk_mtime <= db_modified_at + EXTERNAL_EDIT_DRIFT_TOLERANCE_SECS {
            return None;
        }
        let (disk_title, disk_content) = storage::read_note_file(&file_path).await.ok()?;
        Some(ExternalChanges {
            disk_title,
            disk_content,
            disk_modified_at: disk_mtime,
        })
    }

    pub async fn reload_from_disk(&self, id: &str) -> Result<Note, String> {
        let file_path = storage::find_note_file(&self.storage_path, id).await
            .ok_or_else(|| "note file not found on disk".to_string())?;
        let (disk_title, disk_content) = storage::read_note_file(&file_path).await?;
        
        let existing = self.get(id).await?;
        let effective_title = if disk_title.is_empty() {
            existing
                .map(|n| n.title)
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            disk_title
        };
        self.update(id, &effective_title, &disk_content).await
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Note>, String> {
        let pattern = format!("%{}%", query);
        let db = self.db.clone();
        
        let results = tokio::task::spawn_blocking(move || {
            db.query_map(
                "SELECT id, title, content, created_at, modified_at FROM notes
                 WHERE title LIKE ? OR content LIKE ?
                 ORDER BY modified_at DESC LIMIT 50",
                &[&pattern, &pattern],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
        }).await.map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for (id, title, content, created_at, modified_at) in results {
            let note_tags = self.get_tags(&id).await?;
            notes.push(Note {
                id,
                title,
                content,
                tags: note_tags,
                created_at,
                modified_at,
                external_changes: None,
            });
        }
        Ok(notes)
    }

    pub async fn get_recent(&self, limit: usize) -> Result<Vec<Note>, String> {
        let db = self.db.clone();
        let results = tokio::task::spawn_blocking(move || {
            db.query_map(
                "SELECT id, title, content, created_at, modified_at FROM notes
                 ORDER BY modified_at DESC LIMIT ?",
                &[&(limit as i64)],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
        }).await.map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for (id, title, content, created_at, modified_at) in results {
            let note_tags = self.get_tags(&id).await?;
            notes.push(Note {
                id,
                title,
                content,
                tags: note_tags,
                created_at,
                modified_at,
                external_changes: None,
            });
        }
        Ok(notes)
    }

    async fn get_tags(&self, note_id: &str) -> Result<Vec<String>, String> {
        let id_clone = note_id.to_string();
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db.query_map(
                "SELECT tag FROM note_tags WHERE note_id = ?",
                &[&id_clone as &dyn ToSql],
                |row| row.get(0),
            )
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::path::PathBuf;

    fn manager() -> (NotesManager, TempDir) {
        let db = Arc::new(Database::in_memory().expect("in-memory DB"));
        let dir = TempDir::new().expect("tempdir");
        let mgr = NotesManager::new(db, dir.path().to_path_buf());
        (mgr, dir)
    }

    fn count_files_with_id_prefix(dir: &std::path::Path, id: &str) -> usize {
        let prefix = id.replace("note:", "");
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with(&prefix)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    fn advance_ms_clock() {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    #[tokio::test]
    async fn create_persists_row_to_database() {
        let (mgr, _dir) = manager();
        let note = mgr.create("Title", "Body").await.unwrap();
        let fetched = mgr.get(&note.id).await.unwrap().unwrap();
        assert_eq!(fetched.title, "Title");
        assert_eq!(fetched.content, "Body");
    }

    #[tokio::test]
    async fn create_writes_file_to_disk() {
        let (mgr, dir) = manager();
        let note = mgr.create("My Title", "body").await.unwrap();
        assert_eq!(
            count_files_with_id_prefix(dir.path(), &note.id),
            1,
            "expected exactly one .md file for the new note"
        );
    }

    #[tokio::test]
    async fn create_does_not_leave_tmp_file_after_successful_write() {
        let (mgr, dir) = manager();
        mgr.create("T", "B").await.unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            tmps.is_empty(),
            "expected no .tmp files after successful write, got {tmps:?}"
        );
    }

    #[tokio::test]
    async fn update_does_not_leave_tmp_file() {
        let (mgr, dir) = manager();
        let note = mgr.create("Old", "body").await.unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "New", "body").await.unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "update left tmp files: {tmps:?}");
    }

    #[tokio::test]
    async fn create_file_contents_include_title_header_and_body() {
        let (mgr, dir) = manager();
        let note = mgr.create("My Title", "hello world").await.unwrap();
        let prefix = note.id.replace("note:", "");
        let file = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .unwrap();
        let body = std::fs::read_to_string(file.path()).unwrap();
        assert!(body.contains("# My Title"), "missing H1: {body:?}");
        assert!(body.contains("hello world"), "missing body: {body:?}");
    }

    #[tokio::test]
    async fn create_extracts_hashtags_into_note_tags_table() {
        let (mgr, _dir) = manager();
        let note = mgr
            .create("t", "Meeting notes about #design and #api decisions")
            .await
            .unwrap();
        let mut tags = note.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["api".to_string(), "design".to_string()]);
        let mut fetched_tags = mgr.get(&note.id).await.unwrap().unwrap().tags;
        fetched_tags.sort();
        assert_eq!(fetched_tags, vec!["api".to_string(), "design".to_string()]);
    }

    #[tokio::test]
    async fn create_returns_populated_note_struct() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "B").await.unwrap();
        assert!(note.id.starts_with("note:"));
        assert_eq!(note.title, "T");
        assert_eq!(note.content, "B");
        assert_eq!(note.created_at, note.modified_at);
        assert!(note.created_at > 0);
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_id() {
        let (mgr, _dir) = manager();
        assert!(mgr.get("note:does-not-exist").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_modifies_title_and_content() {
        let (mgr, _dir) = manager();
        let note = mgr.create("Old", "old body").await.unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "New", "new body").await.unwrap();
        let fetched = mgr.get(&note.id).await.unwrap().unwrap();
        assert_eq!(fetched.title, "New");
        assert_eq!(fetched.content, "new body");
    }

    #[tokio::test]
    async fn update_preserves_created_at_but_advances_modified_at() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "B").await.unwrap();
        advance_ms_clock();
        let updated = mgr.update(&note.id, "T2", "B2").await.unwrap();
        assert_eq!(
            updated.created_at, note.created_at,
            "created_at should be preserved across update"
        );
        assert!(updated.modified_at >= note.modified_at);
    }

    #[tokio::test]
    async fn update_replaces_tags_when_content_changes() {
        let (mgr, _dir) = manager();
        let note = mgr.create("t", "Content with #alpha").await.unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "t", "Content with #beta and #gamma")
            .await
            .unwrap();
        let mut tags = mgr.get(&note.id).await.unwrap().unwrap().tags;
        tags.sort();
        assert_eq!(tags, vec!["beta".to_string(), "gamma".to_string()]);
    }

    #[tokio::test]
    async fn update_rewrites_file_at_new_filename_when_title_changes() {
        let (mgr, dir) = manager();
        let note = mgr.create("OldTitle", "body").await.unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "NewTitle", "body").await.unwrap();

        let prefix = note.id.replace("note:", "");
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .collect();
        assert_eq!(
            files.len(),
            1,
            "expected exactly one file after rename; got {files:?}"
        );
        let name = files[0].file_name().to_string_lossy().to_string();
        assert!(name.contains("NewTitle"), "filename should reflect new title: {name}");
        assert!(!name.contains("OldTitle"), "old-title file should be gone: {name}");
    }

    #[tokio::test]
    async fn repeated_title_changes_keep_only_one_file_on_disk() {
        let (mgr, dir) = manager();
        let note = mgr.create("First", "body").await.unwrap();
        for title in ["Second", "Third", "Fourth"] {
            advance_ms_clock();
            mgr.update(&note.id, title, "body").await.unwrap();
        }
        let prefix = note.id.replace("note:", "");
        let count = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count();
        assert_eq!(count, 1, "expected exactly one file after 3 renames");
    }

    #[tokio::test]
    async fn delete_removes_all_stale_files_for_id() {
        let (mgr, dir) = manager();
        let note = mgr.create("t", "b").await.unwrap();
        let prefix = note.id.replace("note:", "");
        let stale = dir.path().join(format!("{prefix}-stale.md"));
        std::fs::write(&stale, "leftover").unwrap();

        mgr.delete(&note.id).await.unwrap();

        let remaining = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count();
        assert_eq!(remaining, 0, "delete should clean all files matching the id");
    }

    #[tokio::test]
    async fn delete_removes_row_from_database() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "B").await.unwrap();
        mgr.delete(&note.id).await.unwrap();
        assert!(mgr.get(&note.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_removes_file_from_disk() {
        let (mgr, dir) = manager();
        let note = mgr.create("T", "B").await.unwrap();
        assert_eq!(count_files_with_id_prefix(dir.path(), &note.id), 1);
        mgr.delete(&note.id).await.unwrap();
        assert_eq!(
            count_files_with_id_prefix(dir.path(), &note.id),
            0,
            "expected file to be deleted"
        );
    }

    #[tokio::test]
    async fn delete_is_ok_for_unknown_id() {
        let (mgr, _dir) = manager();
        assert!(mgr.delete("note:does-not-exist").await.is_ok());
    }

    #[tokio::test]
    async fn search_matches_title_substring() {
        let (mgr, _dir) = manager();
        let note = mgr.create("Meeting notes", "body").await.unwrap();
        advance_ms_clock();
        mgr.create("Shopping list", "other body").await.unwrap();

        let results = mgr.search("meet").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, note.id);
    }

    #[tokio::test]
    async fn search_matches_content_substring() {
        let (mgr, _dir) = manager();
        mgr.create("t1", "contains keyword foobar here").await.unwrap();
        advance_ms_clock();
        mgr.create("t2", "unrelated content").await.unwrap();

        let results = mgr.search("foobar").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "contains keyword foobar here");
    }

    #[tokio::test]
    async fn search_returns_empty_for_no_matches() {
        let (mgr, _dir) = manager();
        mgr.create("Meeting", "body").await.unwrap();
        assert!(mgr.search("xylophone").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_is_case_insensitive_for_ascii() {
        let (mgr, _dir) = manager();
        mgr.create("Meeting", "body").await.unwrap();
        let results = mgr.search("MEETING").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn get_recent_respects_limit() {
        let (mgr, _dir) = manager();
        for i in 0..5 {
            mgr.create(&format!("t{i}"), "b").await.unwrap();
            advance_ms_clock();
        }
        let top2 = mgr.get_recent(2).await.unwrap();
        assert_eq!(top2.len(), 2);
    }

    #[tokio::test]
    async fn get_recent_returns_empty_on_empty_db() {
        let (mgr, _dir) = manager();
        assert!(mgr.get_recent(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_without_external_edit_has_no_external_changes() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "body").await.unwrap();
        let fetched = mgr.get(&note.id).await.unwrap().unwrap();
        assert!(
            fetched.external_changes.is_none(),
            "fresh note should have no external_changes; got: {:?}",
            fetched.external_changes
        );
    }

    #[tokio::test]
    async fn get_detects_external_edit_when_disk_mtime_exceeds_drift_tolerance() {
        let (mgr, dir) = manager();
        let note = mgr.create("T", "original body").await.unwrap();

        let file_path = storage::find_note_file(dir.path(), &note.id).await.unwrap();
        std::fs::write(&file_path, "# T\n\nexternally edited").unwrap();

        use std::sync::Arc;
        use crate::db::Database;
        let db: Arc<Database> = Arc::clone(&mgr.db);
        db.execute(
            "UPDATE notes SET modified_at = ? WHERE id = ?",
            &[&100_i64, &note.id],
        )
        .unwrap();

        let fetched = mgr.get(&note.id).await.unwrap().unwrap();
        let ext = fetched
            .external_changes
            .expect("expected external_changes when file mtime far exceeds DB modified_at");
        assert_eq!(ext.disk_title, "T");
        assert_eq!(ext.disk_content, "externally edited");
        assert!(ext.disk_modified_at > 100);
    }

    #[tokio::test]
    async fn detect_external_changes_missing_file_is_in_sync() {
        let (mgr, dir) = manager();
        let note = mgr.create("T", "body").await.unwrap();
        let file_path = storage::find_note_file(dir.path(), &note.id).await.unwrap();
        std::fs::remove_file(file_path).unwrap();

        let fetched = mgr.get(&note.id).await.unwrap().unwrap();
        assert!(
            fetched.external_changes.is_none(),
            "missing file should be treated as in-sync, not as an external edit"
        );
    }

    #[tokio::test]
    async fn reload_from_disk_copies_disk_content_into_db() {
        let (mgr, dir) = manager();
        let note = mgr.create("Old Title", "old body").await.unwrap();

        let file_path = storage::find_note_file(dir.path(), &note.id).await.unwrap();
        std::fs::write(&file_path, "# New Title\n\nnew body").unwrap();
        advance_ms_clock();

        let reloaded = mgr.reload_from_disk(&note.id).await.unwrap();
        assert_eq!(reloaded.title, "New Title");
        assert_eq!(reloaded.content, "new body");
        assert!(
            reloaded.external_changes.is_none(),
            "after reload, the DB should match disk — external_changes must clear"
        );

        let fetched = mgr.get(&note.id).await.unwrap().unwrap();
        assert_eq!(fetched.title, "New Title");
        assert_eq!(fetched.content, "new body");
    }

    #[tokio::test]
    async fn reload_from_disk_falls_back_to_db_title_when_file_has_no_header() {
        let (mgr, dir) = manager();
        let note = mgr.create("Kept Title", "old body").await.unwrap();

        let file_path = storage::find_note_file(dir.path(), &note.id).await.unwrap();
        std::fs::write(&file_path, "no header, just body lines").unwrap();
        advance_ms_clock();

        let reloaded = mgr.reload_from_disk(&note.id).await.unwrap();
        assert_eq!(reloaded.title, "Kept Title");
        assert_eq!(reloaded.content, "no header, just body lines");
    }

    #[tokio::test]
    async fn reload_from_disk_errors_when_file_missing() {
        let (mgr, dir) = manager();
        let note = mgr.create("T", "body").await.unwrap();
        let file_path = storage::find_note_file(dir.path(), &note.id).await.unwrap();
        std::fs::remove_file(file_path).unwrap();
        assert!(mgr.reload_from_disk(&note.id).await.is_err());
    }

    #[tokio::test]
    async fn create_rolls_back_database_if_file_write_fails() {
        let (mgr, dir) = manager();
        
        let blocked_path = dir.path().join("blocked_dir");
        std::fs::write(&blocked_path, "I am a file, not a directory").unwrap();
        
        let bad_mgr = NotesManager::new(mgr.db.clone(), blocked_path.join("subfolder"));

        let result = bad_mgr.create("Fail", "Content").await;
        assert!(result.is_err(), "expected create to fail due to file-as-directory conflict");

        let count: i64 = bad_mgr.db.query_map("SELECT COUNT(*) FROM notes", &[], |row| row.get(0))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(count, 0, "DB record should have been rolled back");
    }
}
