//! WAT-301: user-defined text snippets.
//!
//! A snippet is a `(trigger, name, expansion)` tuple. Triggers surface
//! the snippet in the unified search pipeline; names are the
//! human-readable label in the result row; expansions are the text
//! pasted into the prior-focused window when the user hits Enter.
//!
//! ## Trigger matching
//!
//! `search(query)` matches queries against both the trigger AND the
//! name, case-insensitively. Users conventionally start triggers with
//! a semicolon (`;addr`, `;sig`) to avoid colliding with app names,
//! but nothing here enforces that — the trigger is just a string.
//!
//! ## Keying
//!
//! Triggers are NOT unique on their own; a user can keep multiple
//! snippets with overlapping triggers (e.g., two different `;sig`
//! variants). The id is the primary key. This trades some "intuitive
//! uniqueness" for simpler CRUD semantics — the UI can enforce
//! uniqueness if we want it.

use crate::db::Database;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    /// What the user types to surface the snippet (e.g., `;addr`).
    pub trigger: String,
    /// Human-readable label shown in the result row.
    pub name: String,
    /// The text that gets pasted on activation.
    pub expansion: String,
    pub created_at: i64,
    pub modified_at: i64,
}

pub struct SnippetsManager {
    db: Arc<Database>,
}

impl SnippetsManager {
    pub fn new(db: Arc<Database>) -> Self {
        SnippetsManager { db }
    }

    pub fn create(&self, trigger: &str, name: &str, expansion: &str) -> Result<Snippet, String> {
        let id = format!("snip:{}", Utc::now().timestamp_millis());
        let now = Utc::now().timestamp();
        self.db
            .execute(
                "INSERT INTO snippets (id, trigger, name, expansion, created_at, modified_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
                &[&id, &trigger, &name, &expansion, &now, &now],
            )
            .map_err(|e| e.to_string())?;
        Ok(Snippet {
            id,
            trigger: trigger.to_string(),
            name: name.to_string(),
            expansion: expansion.to_string(),
            created_at: now,
            modified_at: now,
        })
    }

    pub fn update(
        &self,
        id: &str,
        trigger: &str,
        name: &str,
        expansion: &str,
    ) -> Result<Snippet, String> {
        let now = Utc::now().timestamp();
        self.db
            .execute(
                "UPDATE snippets SET trigger = ?, name = ?, expansion = ?, modified_at = ? WHERE id = ?",
                &[&trigger, &name, &expansion, &now, &id],
            )
            .map_err(|e| e.to_string())?;
        // Read back the full row so the caller gets created_at too.
        self.get(id)?
            .ok_or_else(|| format!("snippet '{id}' not found after update"))
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.db
            .execute("DELETE FROM snippets WHERE id = ?", &[&id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Snippet>, String> {
        let rows = self
            .db
            .query_map(
                "SELECT id, trigger, name, expansion, created_at, modified_at
                 FROM snippets WHERE id = ?",
                &[&id],
                row_to_snippet,
            )
            .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().next())
    }

    pub fn list(&self) -> Result<Vec<Snippet>, String> {
        self.db
            .query_map(
                "SELECT id, trigger, name, expansion, created_at, modified_at
                 FROM snippets ORDER BY LOWER(trigger) ASC",
                &[],
                row_to_snippet,
            )
            .map_err(|e| e.to_string())
    }

    /// Match snippets against a user's query. Returns any snippet whose
    /// trigger OR name contains the query as a case-insensitive
    /// substring. Ordering: exact trigger match first (so `;addr`
    /// beats `;address`), then everything else by modified_at desc so
    /// recent edits stay near the top.
    pub fn search(&self, query: &str) -> Result<Vec<Snippet>, String> {
        let needle = format!("%{}%", query.to_lowercase());
        let exact = query.to_lowercase();
        self.db
            .query_map(
                "SELECT id, trigger, name, expansion, created_at, modified_at
                 FROM snippets
                 WHERE LOWER(trigger) LIKE ? OR LOWER(name) LIKE ?
                 ORDER BY (LOWER(trigger) = ?) DESC, modified_at DESC
                 LIMIT 20",
                &[&needle, &needle, &exact],
                row_to_snippet,
            )
            .map_err(|e| e.to_string())
    }
}

fn row_to_snippet(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    Ok(Snippet {
        id: row.get(0)?,
        trigger: row.get(1)?,
        name: row.get(2)?,
        expansion: row.get(3)?,
        created_at: row.get(4)?,
        modified_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> SnippetsManager {
        SnippetsManager::new(Arc::new(Database::in_memory().expect("in-memory db")))
    }

    /// Ids are derived from `timestamp_millis()`, so two creates in
    /// the same millisecond collide on the PRIMARY KEY. This helper
    /// ticks the clock forward. Production rapid-import is a known
    /// sub-ms collision risk documented on the notes module.
    fn advance_ms_clock() {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    // --- create / get / list ---

    #[test]
    fn create_persists_row_and_returns_populated_snippet() {
        let m = mgr();
        let s = m.create(";addr", "Home Address", "123 Main St").unwrap();
        assert!(s.id.starts_with("snip:"));
        assert_eq!(s.trigger, ";addr");
        assert_eq!(s.name, "Home Address");
        assert_eq!(s.expansion, "123 Main St");
        assert_eq!(s.created_at, s.modified_at);

        // Round-trip via get().
        let fetched = m.get(&s.id).unwrap().unwrap();
        assert_eq!(fetched.expansion, "123 Main St");
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let m = mgr();
        assert!(m.get("snip:nope").unwrap().is_none());
    }

    #[test]
    fn list_orders_alphabetically_by_trigger_case_insensitive() {
        let m = mgr();
        m.create(";zeta", "z", "z").unwrap();
        advance_ms_clock();
        m.create(";Alpha", "a", "a").unwrap();
        advance_ms_clock();
        m.create(";mid", "m", "m").unwrap();

        let all = m.list().unwrap();
        let triggers: Vec<String> = all.iter().map(|s| s.trigger.clone()).collect();
        assert_eq!(triggers, vec![";Alpha".to_string(), ";mid".to_string(), ";zeta".to_string()]);
    }

    // --- update ---

    #[test]
    fn update_modifies_fields_and_bumps_modified_at() {
        let m = mgr();
        let s = m.create("t1", "n1", "e1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let updated = m.update(&s.id, "t2", "n2", "e2").unwrap();
        assert_eq!(updated.trigger, "t2");
        assert_eq!(updated.name, "n2");
        assert_eq!(updated.expansion, "e2");
        assert!(
            updated.modified_at > s.modified_at,
            "modified_at should advance; before={}, after={}",
            s.modified_at,
            updated.modified_at
        );
        assert_eq!(updated.created_at, s.created_at, "created_at must not change");
    }

    #[test]
    fn update_fails_on_unknown_id() {
        let m = mgr();
        assert!(m.update("snip:nope", "t", "n", "e").is_err());
    }

    // --- delete ---

    #[test]
    fn delete_removes_the_row() {
        let m = mgr();
        let s = m.create("t", "n", "e").unwrap();
        m.delete(&s.id).unwrap();
        assert!(m.get(&s.id).unwrap().is_none());
    }

    #[test]
    fn delete_unknown_id_is_ok() {
        // Delete is idempotent — don't error if the row already went
        // away (concurrent UI interactions, refresh races).
        let m = mgr();
        assert!(m.delete("snip:nope").is_ok());
    }

    // --- search ---

    #[test]
    fn search_matches_trigger_substring_case_insensitively() {
        let m = mgr();
        m.create(";addr", "Addr", "...").unwrap();
        advance_ms_clock();
        m.create(";sig", "Sig", "...").unwrap();
        let hits = m.search("ADDR").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].trigger, ";addr");
    }

    #[test]
    fn search_matches_name_substring() {
        let m = mgr();
        m.create("t1", "Home Address", "...").unwrap();
        let hits = m.search("home").unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let m = mgr();
        m.create("t", "n", "e").unwrap();
        assert!(m.search("nothing-matches-this").unwrap().is_empty());
    }

    #[test]
    fn search_orders_exact_trigger_match_ahead_of_prefix_match() {
        let m = mgr();
        m.create(";address", "Longer", "...").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        m.create(";addr", "Exact", "...").unwrap();

        let hits = m.search(";addr").unwrap();
        assert_eq!(hits.len(), 2);
        // Exact match for ";addr" must come first even though the other
        // was created first (more recent modified_at doesn't override
        // the exact-trigger bonus).
        assert_eq!(hits[0].trigger, ";addr");
    }
}
