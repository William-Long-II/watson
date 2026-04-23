pub mod indexer;

use crate::db::Database;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub extension: Option<String>,
    pub size_bytes: Option<i64>,
    pub modified_at: i64,
}

pub struct FileSearchManager {
    db: Arc<Database>,
}

impl FileSearchManager {
    pub fn new(db: Arc<Database>) -> Self {
        FileSearchManager { db }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<FileEntry>, String> {
        let pattern = format!("%{}%", query.to_lowercase());
        self.db
            .query_map(
                "SELECT id, name, path, extension, size_bytes, modified_at FROM files
                 WHERE LOWER(name) LIKE ? OR LOWER(path) LIKE ?
                 ORDER BY modified_at DESC LIMIT ?",
                &[&pattern, &pattern, &(limit as i64)],
                |row| {
                    Ok(FileEntry {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        path: row.get(2)?,
                        extension: row.get(3)?,
                        size_bytes: row.get(4)?,
                        modified_at: row.get(5)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn search_by_extension(&self, ext: &str, limit: usize) -> Result<Vec<FileEntry>, String> {
        let ext_lower = ext.to_lowercase();
        self.db
            .query_map(
                "SELECT id, name, path, extension, size_bytes, modified_at FROM files
                 WHERE extension = ?
                 ORDER BY modified_at DESC LIMIT ?",
                &[&ext_lower, &(limit as i64)],
                |row| {
                    Ok(FileEntry {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        path: row.get(2)?,
                        extension: row.get(3)?,
                        size_bytes: row.get(4)?,
                        modified_at: row.get(5)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }

    pub fn insert(&self, entry: &FileEntry) -> Result<(), String> {
        let now = Utc::now().timestamp();
        self.db
            .execute(
                "INSERT OR REPLACE INTO files (id, name, path, extension, size_bytes, modified_at, indexed_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                &[
                    &entry.id,
                    &entry.name,
                    &entry.path,
                    &entry.extension,
                    &entry.size_bytes,
                    &entry.modified_at,
                    &now,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_by_path(&self, path: &str) -> Result<(), String> {
        self.db
            .execute("DELETE FROM files WHERE path = ?", &[&path])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let empty: &[&dyn rusqlite::ToSql] = &[];
        self.db
            .execute("DELETE FROM files", empty)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_recent(&self, limit: usize) -> Result<Vec<FileEntry>, String> {
        self.db
            .query_map(
                "SELECT id, name, path, extension, size_bytes, modified_at FROM files
                 ORDER BY modified_at DESC LIMIT ?",
                &[&(limit as i64)],
                |row| {
                    Ok(FileEntry {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        path: row.get(2)?,
                        extension: row.get(3)?,
                        size_bytes: row.get(4)?,
                        modified_at: row.get(5)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> FileSearchManager {
        let db = Arc::new(Database::in_memory().expect("in-memory DB"));
        FileSearchManager::new(db)
    }

    fn entry(id: &str, name: &str, path: &str, ext: &str, modified_at: i64) -> FileEntry {
        FileEntry {
            id: id.to_string(),
            name: name.to_string(),
            path: path.to_string(),
            extension: Some(ext.to_string()),
            size_bytes: Some(1024),
            modified_at,
        }
    }

    // --- insert ---

    #[test]
    fn insert_persists_entry() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "report.pdf", "/tmp/report.pdf", "pdf", 100))
            .unwrap();
        let all = mgr.get_recent(10).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "report.pdf");
    }

    #[test]
    fn insert_or_replace_updates_existing_entry_by_id() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "report.pdf", "/old/path.pdf", "pdf", 100))
            .unwrap();
        mgr.insert(&entry("f:1", "report.pdf", "/new/path.pdf", "pdf", 200))
            .unwrap();
        let all = mgr.get_recent(10).unwrap();
        assert_eq!(all.len(), 1, "same id should replace, not duplicate");
        assert_eq!(all[0].path, "/new/path.pdf");
    }

    // --- search by name/path ---

    #[test]
    fn search_matches_name_substring() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "annual-report.pdf", "/docs/annual-report.pdf", "pdf", 100))
            .unwrap();
        mgr.insert(&entry("f:2", "shopping-list.md", "/notes/shopping-list.md", "md", 200))
            .unwrap();
        let results = mgr.search("report", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "f:1");
    }

    #[test]
    fn search_matches_path_substring_even_when_name_does_not() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "readme.md", "/projects/watson/readme.md", "md", 100))
            .unwrap();
        let results = mgr.search("watson", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_is_case_insensitive() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "Report.PDF", "/docs/Report.PDF", "pdf", 100))
            .unwrap();
        assert_eq!(mgr.search("report", 10).unwrap().len(), 1);
        assert_eq!(mgr.search("REPORT", 10).unwrap().len(), 1);
    }

    #[test]
    fn search_respects_limit() {
        let mgr = manager();
        for i in 0..5 {
            mgr.insert(&entry(
                &format!("f:{i}"),
                "match.txt",
                &format!("/p/{i}/match.txt"),
                "txt",
                i,
            ))
            .unwrap();
        }
        let results = mgr.search("match", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn search_returns_empty_for_no_matches() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "a.txt", "/tmp/a.txt", "txt", 100))
            .unwrap();
        assert!(mgr.search("xyz", 10).unwrap().is_empty());
    }

    // --- search by extension ---

    #[test]
    fn search_by_extension_matches_exact_case_insensitive() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "a.md", "/a.md", "md", 100))
            .unwrap();
        mgr.insert(&entry("f:2", "b.txt", "/b.txt", "txt", 200))
            .unwrap();
        let results = mgr.search_by_extension("md", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "f:1");

        // Caller-supplied uppercase is lowercased before matching.
        let upper = mgr.search_by_extension("MD", 10).unwrap();
        assert_eq!(upper.len(), 1);
    }

    #[test]
    fn search_by_extension_does_not_match_partial() {
        // "md" must not match "mdx"; extension query is exact.
        let mgr = manager();
        mgr.insert(&entry("f:1", "a.mdx", "/a.mdx", "mdx", 100))
            .unwrap();
        assert!(mgr.search_by_extension("md", 10).unwrap().is_empty());
    }

    // --- get_recent ---

    #[test]
    fn get_recent_returns_empty_on_empty_db() {
        let mgr = manager();
        assert!(mgr.get_recent(10).unwrap().is_empty());
    }

    #[test]
    fn get_recent_orders_by_modified_at_desc() {
        let mgr = manager();
        mgr.insert(&entry("f:old", "old.txt", "/old.txt", "txt", 100))
            .unwrap();
        mgr.insert(&entry("f:new", "new.txt", "/new.txt", "txt", 200))
            .unwrap();
        let results = mgr.get_recent(10).unwrap();
        assert_eq!(results[0].id, "f:new", "newer file should come first");
        assert_eq!(results[1].id, "f:old");
    }

    #[test]
    fn get_recent_respects_limit() {
        let mgr = manager();
        for i in 0..5 {
            mgr.insert(&entry(
                &format!("f:{i}"),
                "f.txt",
                &format!("/{i}.txt"),
                "txt",
                i,
            ))
            .unwrap();
        }
        assert_eq!(mgr.get_recent(2).unwrap().len(), 2);
    }

    // --- remove_by_path / clear_all ---

    #[test]
    fn remove_by_path_deletes_matching_entry() {
        let mgr = manager();
        mgr.insert(&entry("f:1", "a.txt", "/tmp/a.txt", "txt", 100))
            .unwrap();
        mgr.insert(&entry("f:2", "b.txt", "/tmp/b.txt", "txt", 200))
            .unwrap();
        mgr.remove_by_path("/tmp/a.txt").unwrap();
        let remaining = mgr.get_recent(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "f:2");
    }

    #[test]
    fn remove_by_path_is_ok_when_path_not_found() {
        let mgr = manager();
        assert!(mgr.remove_by_path("/does/not/exist").is_ok());
    }

    #[test]
    fn clear_all_empties_the_table() {
        let mgr = manager();
        for i in 0..3 {
            mgr.insert(&entry(
                &format!("f:{i}"),
                "f.txt",
                &format!("/{i}.txt"),
                "txt",
                i,
            ))
            .unwrap();
        }
        mgr.clear_all().unwrap();
        assert!(mgr.get_recent(10).unwrap().is_empty());
    }
}
