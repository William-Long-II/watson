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
