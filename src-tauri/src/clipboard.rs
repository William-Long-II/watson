use arboard::Clipboard;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: String,
    pub preview: String,
    pub timestamp: DateTime<Utc>,
}

pub struct ClipboardManager {
    history: Arc<Mutex<Vec<ClipboardEntry>>>,
    max_entries: usize,
    last_content: Arc<Mutex<String>>,
}

impl ClipboardManager {
    pub fn new(max_entries: usize) -> Self {
        ClipboardManager {
            history: Arc::new(Mutex::new(Vec::new())),
            max_entries,
            last_content: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn start_monitoring(&self) {
        let history = Arc::clone(&self.history);
        let last_content = Arc::clone(&self.last_content);
        let max_entries = self.max_entries;

        thread::spawn(move || {
            let mut clipboard = match Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to access clipboard: {}", e);
                    return;
                }
            };

            loop {
                if let Ok(text) = clipboard.get_text() {
                    if !text.is_empty() {
                        let mut last = last_content.lock().unwrap();
                        if *last != text {
                            *last = text.clone();
                            drop(last);

                            let preview = text
                                .chars()
                                .take(100)
                                .collect::<String>()
                                .replace('\n', " ")
                                .replace('\r', "");

                            let entry = ClipboardEntry {
                                id: format!("clip:{}", Utc::now().timestamp_millis()),
                                content: text,
                                preview,
                                timestamp: Utc::now(),
                            };

                            let mut hist = history.lock().unwrap();

                            // Remove duplicate if exists
                            hist.retain(|e| e.content != entry.content);

                            // Add to front
                            hist.insert(0, entry);

                            // Trim to max size
                            hist.truncate(max_entries);
                        }
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
    }

    pub fn get_history(&self) -> Vec<ClipboardEntry> {
        self.history.lock().unwrap().clone()
    }

    pub fn search_history(&self, query: &str) -> Vec<ClipboardEntry> {
        let query_lower = query.to_lowercase();
        self.history
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .cloned()
            .collect()
    }

    pub fn clear_history(&self) {
        self.history.lock().unwrap().clear();
    }

    pub fn copy_to_clipboard(&self, content: &str) -> Result<(), String> {
        let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.set_text(content).map_err(|e| e.to_string())?;

        // Update last_content to avoid re-adding
        *self.last_content.lock().unwrap() = content.to_string();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(content: &str) -> ClipboardEntry {
        ClipboardEntry {
            id: format!("clip:{content}"),
            content: content.to_string(),
            preview: content.chars().take(100).collect(),
            timestamp: Utc::now(),
        }
    }

    /// Push entries directly into the history mutex, bypassing the monitor
    /// thread. Tests exercising the history API don't need OS clipboard
    /// access or the start_monitoring() spawn.
    fn push_entries(mgr: &ClipboardManager, items: &[&str]) {
        let mut hist = mgr.history.lock().unwrap();
        for s in items {
            hist.push(entry(s));
        }
    }

    // --- constructor / initial state ---

    #[test]
    fn new_has_empty_history() {
        let mgr = ClipboardManager::new(50);
        assert!(mgr.get_history().is_empty());
    }

    // --- get_history ---

    #[test]
    fn get_history_returns_entries_in_stored_order() {
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["first", "second", "third"]);
        let hist = mgr.get_history();
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].content, "first");
        assert_eq!(hist[2].content, "third");
    }

    #[test]
    fn get_history_returns_snapshot_not_live_reference() {
        // Mutating the returned Vec must not affect the manager's state —
        // get_history takes a lock and clones.
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["a", "b"]);
        let mut snapshot = mgr.get_history();
        snapshot.clear();
        assert_eq!(
            mgr.get_history().len(),
            2,
            "internal history should be unaffected by external mutation"
        );
    }

    // --- search_history ---

    #[test]
    fn search_history_matches_content_substring() {
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["hello world", "foobar", "hello again"]);
        let results = mgr.search_history("hello");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_history_is_case_insensitive() {
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["Hello World"]);
        assert_eq!(mgr.search_history("hello").len(), 1);
        assert_eq!(mgr.search_history("WORLD").len(), 1);
    }

    #[test]
    fn search_history_returns_empty_on_no_match() {
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["foo", "bar"]);
        assert!(mgr.search_history("xylophone").is_empty());
    }

    #[test]
    fn search_history_empty_query_matches_everything() {
        // Empty query becomes the pattern "" which is a substring of every
        // string. Pin the current contract.
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["a", "b", "c"]);
        assert_eq!(mgr.search_history("").len(), 3);
    }

    // --- clear_history ---

    #[test]
    fn clear_history_empties_state() {
        let mgr = ClipboardManager::new(50);
        push_entries(&mgr, &["a", "b", "c"]);
        mgr.clear_history();
        assert!(mgr.get_history().is_empty());
    }

    #[test]
    fn clear_history_is_ok_on_empty_manager() {
        let mgr = ClipboardManager::new(50);
        mgr.clear_history();
        assert!(mgr.get_history().is_empty());
    }
}
