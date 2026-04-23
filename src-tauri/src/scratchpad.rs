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

    fn manager() -> ScratchpadManager {
        let db = Arc::new(Database::in_memory().expect("in-memory db should open"));
        ScratchpadManager::new(db)
    }

    #[test]
    fn new_initializes_empty_scratchpad_row() {
        let manager = manager();
        let pad = manager.get().expect("scratchpad row should exist after new()");
        assert_eq!(pad.content, "");
    }

    #[test]
    fn set_then_get_round_trips_content() {
        let manager = manager();
        manager.set("hello world").unwrap();
        assert_eq!(manager.get().unwrap().content, "hello world");
    }

    #[test]
    fn clear_resets_content_to_empty_string() {
        let manager = manager();
        manager.set("anything").unwrap();
        manager.clear().unwrap();
        assert_eq!(manager.get().unwrap().content, "");
    }

    #[test]
    fn set_preserves_unicode_and_newlines() {
        let manager = manager();
        let content = "line1\nlíñé2 — 日本語 🧪\nline3";
        manager.set(content).unwrap();
        assert_eq!(manager.get().unwrap().content, content);
    }

    #[test]
    fn set_handles_large_content() {
        let manager = manager();
        let content = "x".repeat(100_000);
        manager.set(&content).unwrap();
        assert_eq!(manager.get().unwrap().content.len(), 100_000);
    }

    #[test]
    fn concurrent_writes_serialize_without_data_loss_or_deadlock() {
        // WAT-104 / R-13: the scratchpad is a single SQLite row guarded by
        // `Database`'s `Mutex<Connection>`. Concurrent calls to `set()` must
        // serialize correctly — no deadlock, no panic, no partial writes —
        // and the final stored value must be one of the values we wrote
        // (never some interleaved garbage or the initial empty string).
        //
        // This also exercises the `Arc<ScratchpadManager>` share pattern
        // that the Tauri command layer uses in prod; if the manager stopped
        // being `Send + Sync`, this test would fail to compile.

        const WRITERS: usize = 8;
        const WRITES_PER_THREAD: usize = 25;

        let manager = Arc::new(manager());

        let mut handles = Vec::with_capacity(WRITERS);
        for thread_id in 0..WRITERS {
            let mgr = Arc::clone(&manager);
            handles.push(std::thread::spawn(move || {
                for write_id in 0..WRITES_PER_THREAD {
                    let content = format!("t{thread_id}-w{write_id}");
                    mgr.set(&content)
                        .expect("concurrent set must not fail");
                }
            }));
        }

        for h in handles {
            h.join().expect("writer thread panicked");
        }

        // Final state must be *some* legal value from one of the writers —
        // not an empty string (the initial state), not corrupted.
        let final_content = manager.get().expect("get after concurrent writes").content;
        assert!(
            final_content.starts_with('t'),
            "final content {final_content:?} doesn't match any writer's pattern — \
             a write was lost or the column was clobbered"
        );
    }
}
