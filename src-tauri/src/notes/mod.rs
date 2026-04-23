pub mod storage;
pub mod tags;

use crate::db::Database;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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

    pub fn create(&self, title: &str, content: &str) -> Result<Note, String> {
        let id = format!("note:{}", Utc::now().timestamp_millis());
        let now = Utc::now().timestamp();
        let extracted_tags = tags::extract_tags(content);

        // Insert into database
        self.db
            .execute(
                "INSERT INTO notes (id, title, content, created_at, modified_at) VALUES (?, ?, ?, ?, ?)",
                &[&id, &title, &content, &now, &now],
            )
            .map_err(|e| e.to_string())?;

        // Insert tags
        for tag in &extracted_tags {
            self.db
                .execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag) VALUES (?, ?)",
                    &[&id, tag],
                )
                .ok();
        }

        // Write to file
        storage::write_note_file(&self.storage_path, &id, title, content)?;

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

    pub fn update(&self, id: &str, title: &str, content: &str) -> Result<Note, String> {
        let now = Utc::now().timestamp();
        let extracted_tags = tags::extract_tags(content);

        // Update database
        self.db
            .execute(
                "UPDATE notes SET title = ?, content = ?, modified_at = ? WHERE id = ?",
                &[&title, &content, &now, &id],
            )
            .map_err(|e| e.to_string())?;

        // Update tags
        self.db
            .execute("DELETE FROM note_tags WHERE note_id = ?", &[&id])
            .ok();
        for tag in &extracted_tags {
            self.db
                .execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag) VALUES (?, ?)",
                    &[&id, tag],
                )
                .ok();
        }

        // Update file
        storage::write_note_file(&self.storage_path, id, title, content)?;

        // Get created_at
        let created_at = self
            .db
            .query_map(
                "SELECT created_at FROM notes WHERE id = ?",
                &[&id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .unwrap_or(now);

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

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.db
            .execute("DELETE FROM notes WHERE id = ?", &[&id])
            .map_err(|e| e.to_string())?;
        storage::delete_note_file(&self.storage_path, id)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Note>, String> {
        let notes = self
            .db
            .query_map(
                "SELECT id, title, content, created_at, modified_at FROM notes WHERE id = ?",
                &[&id],
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
            .map_err(|e| e.to_string())?;

        if let Some((id, title, content, created_at, modified_at)) = notes.into_iter().next() {
            let note_tags = self.get_tags(&id)?;
            // WAT-204: check whether the on-disk .md was modified
            // outside Watson since our last DB update. This runs only on
            // `get()` (typically the open-note flow) — stat-per-result
            // during search is too expensive and doesn't help.
            let external_changes = self.detect_external_changes(&id, modified_at);
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

    /// Returns `Some(ExternalChanges)` when the on-disk file is measurably
    /// newer than the DB's `modified_at`. A missing file — unusual but
    /// possible if the user deleted it manually — is treated as "in sync"
    /// (the DB remains source of truth; next `update()` will recreate).
    fn detect_external_changes(&self, id: &str, db_modified_at: i64) -> Option<ExternalChanges> {
        let file_path = storage::find_note_file(&self.storage_path, id)?;
        let disk_mtime = storage::file_modified_at(&file_path)?;
        if disk_mtime <= db_modified_at + EXTERNAL_EDIT_DRIFT_TOLERANCE_SECS {
            return None;
        }
        let (disk_title, disk_content) = storage::read_note_file(&file_path).ok()?;
        Some(ExternalChanges {
            disk_title,
            disk_content,
            disk_modified_at: disk_mtime,
        })
    }

    /// WAT-204: adopt the on-disk version — read the file, parse, and
    /// write the parsed title/content into the DB via `update()`. The
    /// resulting `Note` has `external_changes: None` (caller is now in
    /// sync).
    pub fn reload_from_disk(&self, id: &str) -> Result<Note, String> {
        let file_path = storage::find_note_file(&self.storage_path, id)
            .ok_or_else(|| "note file not found on disk".to_string())?;
        let (disk_title, disk_content) = storage::read_note_file(&file_path)?;
        // If the external editor stripped the "# title" header, fall back
        // to the DB's current title so the note keeps an identifier.
        let effective_title = if disk_title.is_empty() {
            self.get(id)?
                .map(|n| n.title)
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            disk_title
        };
        self.update(id, &effective_title, &disk_content)
    }

    pub fn search(&self, query: &str) -> Result<Vec<Note>, String> {
        let pattern = format!("%{}%", query);
        let results = self
            .db
            .query_map(
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
            .map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for (id, title, content, created_at, modified_at) in results {
            let note_tags = self.get_tags(&id)?;
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

    pub fn get_recent(&self, limit: usize) -> Result<Vec<Note>, String> {
        let results = self
            .db
            .query_map(
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
            .map_err(|e| e.to_string())?;

        let mut notes = Vec::new();
        for (id, title, content, created_at, modified_at) in results {
            let note_tags = self.get_tags(&id)?;
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

    fn get_tags(&self, note_id: &str) -> Result<Vec<String>, String> {
        self.db
            .query_map(
                "SELECT tag FROM note_tags WHERE note_id = ?",
                &[&note_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

    /// Sleep just long enough for `timestamp_millis()` to advance.
    /// Note IDs are currently derived from ms timestamps, so two creates
    /// in the same millisecond collide on the TEXT PRIMARY KEY. This is
    /// a real (sub-ms rate) production bug but unlikely interactively;
    /// tracked as follow-up risk. In the meantime, tests that create
    /// multiple notes must space them apart.
    fn advance_ms_clock() {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    // --- create ---

    #[test]
    fn create_persists_row_to_database() {
        let (mgr, _dir) = manager();
        let note = mgr.create("Title", "Body").unwrap();
        let fetched = mgr.get(&note.id).unwrap().unwrap();
        assert_eq!(fetched.title, "Title");
        assert_eq!(fetched.content, "Body");
    }

    #[test]
    fn create_writes_file_to_disk() {
        let (mgr, dir) = manager();
        let note = mgr.create("My Title", "body").unwrap();
        assert_eq!(
            count_files_with_id_prefix(dir.path(), &note.id),
            1,
            "expected exactly one .md file for the new note"
        );
    }

    #[test]
    fn create_does_not_leave_tmp_file_after_successful_write() {
        // R-05 atomic-write contract: the .tmp sibling must not survive a
        // successful write. If it did, a later reader could find two files
        // for one note or see a half-written tmp.
        let (mgr, dir) = manager();
        mgr.create("T", "B").unwrap();
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

    #[test]
    fn update_does_not_leave_tmp_file() {
        let (mgr, dir) = manager();
        let note = mgr.create("Old", "body").unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "New", "body").unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "update left tmp files: {tmps:?}");
    }

    #[test]
    fn create_file_contents_include_title_header_and_body() {
        let (mgr, dir) = manager();
        let note = mgr.create("My Title", "hello world").unwrap();
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

    #[test]
    fn create_extracts_hashtags_into_note_tags_table() {
        let (mgr, _dir) = manager();
        let note = mgr
            .create("t", "Meeting notes about #design and #api decisions")
            .unwrap();
        let mut tags = note.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["api".to_string(), "design".to_string()]);
        // Fetched via get() — round-trips through the note_tags table.
        let mut fetched_tags = mgr.get(&note.id).unwrap().unwrap().tags;
        fetched_tags.sort();
        assert_eq!(fetched_tags, vec!["api".to_string(), "design".to_string()]);
    }

    #[test]
    fn create_returns_populated_note_struct() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "B").unwrap();
        assert!(note.id.starts_with("note:"));
        assert_eq!(note.title, "T");
        assert_eq!(note.content, "B");
        assert_eq!(note.created_at, note.modified_at);
        assert!(note.created_at > 0);
    }

    // --- get ---

    #[test]
    fn get_returns_none_for_unknown_id() {
        let (mgr, _dir) = manager();
        assert!(mgr.get("note:does-not-exist").unwrap().is_none());
    }

    // --- update ---

    #[test]
    fn update_modifies_title_and_content() {
        let (mgr, _dir) = manager();
        let note = mgr.create("Old", "old body").unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "New", "new body").unwrap();
        let fetched = mgr.get(&note.id).unwrap().unwrap();
        assert_eq!(fetched.title, "New");
        assert_eq!(fetched.content, "new body");
    }

    #[test]
    fn update_preserves_created_at_but_advances_modified_at() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "B").unwrap();
        advance_ms_clock();
        // timestamp() returns seconds; within tight loop, created_at may
        // equal modified_at after a sub-second update. That's still a
        // valid outcome — we assert only that created_at is preserved.
        let updated = mgr.update(&note.id, "T2", "B2").unwrap();
        assert_eq!(
            updated.created_at, note.created_at,
            "created_at should be preserved across update"
        );
        assert!(updated.modified_at >= note.modified_at);
    }

    #[test]
    fn update_replaces_tags_when_content_changes() {
        let (mgr, _dir) = manager();
        let note = mgr.create("t", "Content with #alpha").unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "t", "Content with #beta and #gamma")
            .unwrap();
        let mut tags = mgr.get(&note.id).unwrap().unwrap().tags;
        tags.sort();
        assert_eq!(tags, vec!["beta".to_string(), "gamma".to_string()]);
    }

    #[test]
    fn update_rewrites_file_at_new_filename_when_title_changes() {
        let (mgr, dir) = manager();
        let note = mgr.create("OldTitle", "body").unwrap();
        advance_ms_clock();
        mgr.update(&note.id, "NewTitle", "body").unwrap();

        // Invariant (R-05 partial mitigation): exactly one .md file per
        // note id on disk. The file carries the new title; the old-title
        // file has been cleaned up by write_note_file::cleanup_stale_files_for_id.
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

    #[test]
    fn repeated_title_changes_keep_only_one_file_on_disk() {
        // Defensive: even after several renames, exactly one file survives.
        let (mgr, dir) = manager();
        let note = mgr.create("First", "body").unwrap();
        for title in ["Second", "Third", "Fourth"] {
            advance_ms_clock();
            mgr.update(&note.id, title, "body").unwrap();
        }
        let prefix = note.id.replace("note:", "");
        let count = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count();
        assert_eq!(count, 1, "expected exactly one file after 3 renames");
    }

    #[test]
    fn delete_removes_all_stale_files_for_id() {
        // Document the delete_note_file contract: it cleans up ALL files
        // matching the id prefix, not just the first.
        let (mgr, dir) = manager();
        let note = mgr.create("t", "b").unwrap();
        // Manually plant a stale file to simulate a pre-fix-era orphan.
        let prefix = note.id.replace("note:", "");
        let stale = dir.path().join(format!("{prefix}-stale.md"));
        std::fs::write(&stale, "leftover").unwrap();

        mgr.delete(&note.id).unwrap();

        let remaining = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count();
        assert_eq!(remaining, 0, "delete should clean all files matching the id");
    }

    // --- delete ---

    #[test]
    fn delete_removes_row_from_database() {
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "B").unwrap();
        mgr.delete(&note.id).unwrap();
        assert!(mgr.get(&note.id).unwrap().is_none());
    }

    #[test]
    fn delete_removes_file_from_disk() {
        let (mgr, dir) = manager();
        let note = mgr.create("T", "B").unwrap();
        assert_eq!(count_files_with_id_prefix(dir.path(), &note.id), 1);
        mgr.delete(&note.id).unwrap();
        assert_eq!(
            count_files_with_id_prefix(dir.path(), &note.id),
            0,
            "expected file to be deleted"
        );
    }

    #[test]
    fn delete_is_ok_for_unknown_id() {
        // Idempotency: deleting a nonexistent note is not an error.
        let (mgr, _dir) = manager();
        assert!(mgr.delete("note:does-not-exist").is_ok());
    }

    // --- search ---

    #[test]
    fn search_matches_title_substring() {
        let (mgr, _dir) = manager();
        let note = mgr.create("Meeting notes", "body").unwrap();
        advance_ms_clock();
        mgr.create("Shopping list", "other body").unwrap();

        let results = mgr.search("meet").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, note.id);
    }

    #[test]
    fn search_matches_content_substring() {
        let (mgr, _dir) = manager();
        mgr.create("t1", "contains keyword foobar here").unwrap();
        advance_ms_clock();
        mgr.create("t2", "unrelated content").unwrap();

        let results = mgr.search("foobar").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "contains keyword foobar here");
    }

    #[test]
    fn search_returns_empty_for_no_matches() {
        let (mgr, _dir) = manager();
        mgr.create("Meeting", "body").unwrap();
        assert!(mgr.search("xylophone").unwrap().is_empty());
    }

    #[test]
    fn search_is_case_insensitive_for_ascii() {
        // SQLite LIKE is case-insensitive for ASCII by default — pin it.
        let (mgr, _dir) = manager();
        mgr.create("Meeting", "body").unwrap();
        let results = mgr.search("MEETING").unwrap();
        assert_eq!(results.len(), 1);
    }

    // --- get_recent ---

    #[test]
    fn get_recent_respects_limit() {
        let (mgr, _dir) = manager();
        for i in 0..5 {
            mgr.create(&format!("t{i}"), "b").unwrap();
            advance_ms_clock();
        }
        let top2 = mgr.get_recent(2).unwrap();
        assert_eq!(top2.len(), 2);
    }

    #[test]
    fn get_recent_returns_empty_on_empty_db() {
        let (mgr, _dir) = manager();
        assert!(mgr.get_recent(10).unwrap().is_empty());
    }

    // --- WAT-204: external-edit detection on get() ---

    #[test]
    fn get_without_external_edit_has_no_external_changes() {
        // Normal flow: create, get — mtime and modified_at align within
        // drift tolerance. external_changes must be None.
        let (mgr, _dir) = manager();
        let note = mgr.create("T", "body").unwrap();
        let fetched = mgr.get(&note.id).unwrap().unwrap();
        assert!(
            fetched.external_changes.is_none(),
            "fresh note should have no external_changes; got: {:?}",
            fetched.external_changes
        );
    }

    #[test]
    fn get_detects_external_edit_when_disk_mtime_exceeds_drift_tolerance() {
        // Simulate: a note whose DB `modified_at` is well in the past,
        // but whose on-disk file was just written (mtime ≈ now). The
        // time gap exceeds drift tolerance, so `get()` must report
        // external_changes with the disk content.
        let (mgr, dir) = manager();
        let note = mgr.create("T", "original body").unwrap();

        // Rewrite the file with new content.
        let file_path = storage::find_note_file(dir.path(), &note.id).unwrap();
        std::fs::write(&file_path, "# T\n\nexternally edited").unwrap();

        // Force the DB modified_at far into the past so the file's mtime
        // (which is approximately now) is well beyond drift tolerance.
        // Direct SQL is the minimum-surface way to test this.
        use std::sync::Arc;
        use crate::db::Database;
        let db: Arc<Database> = Arc::clone(&mgr.db);
        db.execute(
            "UPDATE notes SET modified_at = ? WHERE id = ?",
            &[&100_i64, &note.id],
        )
        .unwrap();

        let fetched = mgr.get(&note.id).unwrap().unwrap();
        let ext = fetched
            .external_changes
            .expect("expected external_changes when file mtime far exceeds DB modified_at");
        assert_eq!(ext.disk_title, "T");
        assert_eq!(ext.disk_content, "externally edited");
        assert!(ext.disk_modified_at > 100);
    }

    #[test]
    fn detect_external_changes_missing_file_is_in_sync() {
        // If someone deleted the .md file manually, `get()` must not
        // panic or report a bogus external change; DB remains source of
        // truth. The next update() will recreate the file.
        let (mgr, dir) = manager();
        let note = mgr.create("T", "body").unwrap();
        let file_path = storage::find_note_file(dir.path(), &note.id).unwrap();
        std::fs::remove_file(file_path).unwrap();

        let fetched = mgr.get(&note.id).unwrap().unwrap();
        assert!(
            fetched.external_changes.is_none(),
            "missing file should be treated as in-sync, not as an external edit"
        );
    }

    #[test]
    fn reload_from_disk_copies_disk_content_into_db() {
        // End-to-end: user externally edited, then chose "Use disk"
        // in the reconcile dialog. reload_from_disk must parse the file
        // and write its content back to the DB, leaving Watson in sync.
        let (mgr, dir) = manager();
        let note = mgr.create("Old Title", "old body").unwrap();

        // External edit: rewrite file with new content + new title.
        let file_path = storage::find_note_file(dir.path(), &note.id).unwrap();
        std::fs::write(&file_path, "# New Title\n\nnew body").unwrap();
        advance_ms_clock();

        let reloaded = mgr.reload_from_disk(&note.id).unwrap();
        assert_eq!(reloaded.title, "New Title");
        assert_eq!(reloaded.content, "new body");
        assert!(
            reloaded.external_changes.is_none(),
            "after reload, the DB should match disk — external_changes must clear"
        );

        // Fetch again to confirm persistence.
        let fetched = mgr.get(&note.id).unwrap().unwrap();
        assert_eq!(fetched.title, "New Title");
        assert_eq!(fetched.content, "new body");
    }

    #[test]
    fn reload_from_disk_falls_back_to_db_title_when_file_has_no_header() {
        // If an external editor stripped the "# title" line, the file
        // parse returns an empty title. Rather than blanking the note's
        // title in the DB, keep the current DB title.
        let (mgr, dir) = manager();
        let note = mgr.create("Kept Title", "old body").unwrap();

        let file_path = storage::find_note_file(dir.path(), &note.id).unwrap();
        std::fs::write(&file_path, "no header, just body lines").unwrap();
        advance_ms_clock();

        let reloaded = mgr.reload_from_disk(&note.id).unwrap();
        assert_eq!(reloaded.title, "Kept Title");
        assert_eq!(reloaded.content, "no header, just body lines");
    }

    #[test]
    fn reload_from_disk_errors_when_file_missing() {
        let (mgr, dir) = manager();
        let note = mgr.create("T", "body").unwrap();
        let file_path = storage::find_note_file(dir.path(), &note.id).unwrap();
        std::fs::remove_file(file_path).unwrap();
        assert!(mgr.reload_from_disk(&note.id).is_err());
    }
}
