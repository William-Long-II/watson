use arboard::Clipboard;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// WAT-106: cooperative shutdown flag for the monitor thread. The loop
    /// checks this between every poll and inside the sleep chunks so clean
    /// exit takes at most ~50ms after `shutdown()` is called.
    shutdown: Arc<AtomicBool>,
    /// WAT-106: handle to the monitor thread, held so `shutdown()` / `Drop`
    /// can join cleanly instead of letting the thread outlive the manager.
    monitor_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl ClipboardManager {
    pub fn new(max_entries: usize) -> Self {
        ClipboardManager {
            history: Arc::new(Mutex::new(Vec::new())),
            max_entries,
            last_content: Arc::new(Mutex::new(String::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
            monitor_handle: Mutex::new(None),
        }
    }

    /// Spawn the production monitor thread backed by the OS clipboard.
    pub fn start_monitoring(&self) {
        let clipboard = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to access clipboard: {}", e);
                return;
            }
        };
        // `Clipboard` is not `Sync`, so wrap it in a `Mutex` to satisfy the
        // `FnMut + Send` bound on the poller closure.
        let clipboard = Mutex::new(clipboard);
        self.start_monitoring_with_poller(move || {
            clipboard
                .lock()
                .unwrap()
                .get_text()
                .ok()
                .filter(|s| !s.is_empty())
        });
    }

    /// Spawn a monitor thread that reads clipboard text from `poll()` on a
    /// 500ms cadence. Extracted from `start_monitoring` so tests can inject a
    /// deterministic poller instead of depending on a real OS clipboard.
    pub fn start_monitoring_with_poller<F>(&self, mut poll: F)
    where
        F: FnMut() -> Option<String> + Send + 'static,
    {
        let history = Arc::clone(&self.history);
        let last_content = Arc::clone(&self.last_content);
        let shutdown = Arc::clone(&self.shutdown);
        let max_entries = self.max_entries;

        let handle = thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                if let Some(text) = poll() {
                    record_entry(&history, &last_content, &text, max_entries);
                }
                // 10 × 50ms = 500ms total cadence, but responsive to shutdown
                // within one 50ms tick.
                for _ in 0..10 {
                    if shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });

        *self.monitor_handle.lock().unwrap() = Some(handle);
    }

    /// Request the monitor thread exit and join it. Idempotent; safe to call
    /// when monitoring was never started.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.monitor_handle.lock().unwrap().take() {
            let _ = handle.join();
        }
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

impl Drop for ClipboardManager {
    fn drop(&mut self) {
        // WAT-106: ensure the monitor thread doesn't outlive the manager.
        self.shutdown();
    }
}

/// Record a new clipboard text entry into `history` if it differs from the
/// last-seen content. Shared between the production and test monitor loops.
fn record_entry(
    history: &Arc<Mutex<Vec<ClipboardEntry>>>,
    last_content: &Arc<Mutex<String>>,
    text: &str,
    max_entries: usize,
) {
    let mut last = last_content.lock().unwrap();
    if *last == text {
        return;
    }
    *last = text.to_string();
    drop(last);

    let preview = text
        .chars()
        .take(100)
        .collect::<String>()
        .replace('\n', " ")
        .replace('\r', "");

    let entry = ClipboardEntry {
        id: format!("clip:{}", Utc::now().timestamp_millis()),
        content: text.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

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

    // --- WAT-106: cooperative shutdown ---

    #[test]
    fn shutdown_is_idempotent_when_never_started() {
        // A manager whose monitor thread was never spawned must still accept
        // shutdown() cleanly — nothing to join, no panic.
        let mgr = ClipboardManager::new(50);
        mgr.shutdown();
        mgr.shutdown();
    }

    #[test]
    fn shutdown_stops_a_running_monitor_quickly() {
        // Inject a stub poller so the test doesn't need an OS clipboard. The
        // poller bumps a counter each tick so we can assert the loop was
        // actually running before we asked it to stop.
        let mgr = ClipboardManager::new(50);
        let poll_count = Arc::new(AtomicUsize::new(0));
        let pc = Arc::clone(&poll_count);
        mgr.start_monitoring_with_poller(move || {
            pc.fetch_add(1, Ordering::Relaxed);
            None
        });

        // Give the loop a couple of ticks to prove it's alive.
        thread::sleep(Duration::from_millis(150));
        assert!(
            poll_count.load(Ordering::Relaxed) >= 1,
            "poller should run at least once before shutdown"
        );

        let start = Instant::now();
        mgr.shutdown();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "shutdown took {elapsed:?}, expected <500ms; the monitor thread did not exit cleanly"
        );
    }

    #[test]
    fn drop_joins_the_monitor_thread() {
        // Drop must shut down the monitor. If it didn't, the poller closure
        // would continue running after the manager is gone and mutate the
        // shared counter forever. We assert drop blocks until the thread has
        // stopped by snapshotting the counter right after drop and again
        // 100ms later.
        let poll_count = Arc::new(AtomicUsize::new(0));
        let pc = Arc::clone(&poll_count);
        {
            let mgr = ClipboardManager::new(50);
            mgr.start_monitoring_with_poller(move || {
                pc.fetch_add(1, Ordering::Relaxed);
                None
            });
            thread::sleep(Duration::from_millis(100));
        } // Drop runs here; must join.

        let snapshot = poll_count.load(Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100));
        let after = poll_count.load(Ordering::Relaxed);
        assert_eq!(
            snapshot, after,
            "poller counter advanced after Drop; the monitor thread was not joined"
        );
    }

    #[test]
    fn monitor_records_entries_via_injected_poller() {
        // End-to-end smoke: poller feeds text; after a tick the history shows
        // it. Proves the refactor preserved the production behavior.
        let mgr = ClipboardManager::new(50);
        let texts = Arc::new(Mutex::new(vec!["hello".to_string()]));
        let texts_handle = Arc::clone(&texts);
        mgr.start_monitoring_with_poller(move || texts_handle.lock().unwrap().pop());

        thread::sleep(Duration::from_millis(150));
        mgr.shutdown();

        let hist = mgr.get_history();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].content, "hello");
    }
}
