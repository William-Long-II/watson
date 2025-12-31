use crate::db::Database;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scratchpad {
    pub content: String,
    pub modified_at: i64,
}

pub struct ScratchpadManager {
    db: Arc<Database>,
}

impl ScratchpadManager {
    pub fn new(db: Arc<Database>) -> Self {
        // Initialize scratchpad row if not exists
        let _ = db.execute(
            "INSERT OR IGNORE INTO scratchpad (id, content, modified_at) VALUES (1, '', ?)",
            &[&Utc::now().timestamp()],
        );
        ScratchpadManager { db }
    }

    pub fn get(&self) -> Result<Scratchpad, String> {
        let results = self
            .db
            .query_map(
                "SELECT content, modified_at FROM scratchpad WHERE id = 1",
                &[],
                |row| {
                    Ok(Scratchpad {
                        content: row.get(0)?,
                        modified_at: row.get(1)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        results.into_iter().next().ok_or_else(|| "Scratchpad not found".to_string())
    }

    pub fn set(&self, content: &str) -> Result<(), String> {
        self.db
            .execute(
                "UPDATE scratchpad SET content = ?, modified_at = ? WHERE id = 1",
                &[&content, &Utc::now().timestamp()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        self.set("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn test_scratchpad_get_set() {
        let db = Database::new().unwrap();
        let manager = ScratchpadManager::new(Arc::new(db));

        // Initially empty
        let pad = manager.get().unwrap();
        assert_eq!(pad.content, "");

        // Set content
        manager.set("test content").unwrap();
        let pad = manager.get().unwrap();
        assert_eq!(pad.content, "test content");

        // Clear
        manager.clear().unwrap();
        let pad = manager.get().unwrap();
        assert_eq!(pad.content, "");
    }
}
