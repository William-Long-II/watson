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
}

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
            Ok(Some(Note {
                id,
                title,
                content,
                tags: note_tags,
                created_at,
                modified_at,
            }))
        } else {
            Ok(None)
        }
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
