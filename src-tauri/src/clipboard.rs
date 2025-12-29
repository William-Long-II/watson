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
