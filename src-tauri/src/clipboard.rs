use crate::db::Database;
use arboard::Clipboard;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: String,
    pub content: String,
    pub preview: String,
    pub timestamp: DateTime<Utc>,
    /// WAT-303: pinned entries survive `clear_history`, sort above
    /// unpinned in `get_history`, and persist across app restarts (the
    /// only clipboard entries that do).
    #[serde(default)]
    pub pinned: bool,
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
    /// WAT-303: persistence for pinned entries. Regular history is
    /// in-memory only; only pins get the cost of a DB row.
    db: Arc<Database>,
    /// WAT-303: compiled ignore patterns. Entries whose content matches
    /// any pattern are silently dropped from the monitor loop — a thin
    /// privacy filter for password-manager output, tokens, etc.
    /// Wrapped in an RwLock so a settings update can hot-reload without
    /// restarting the monitor.
    ignore_patterns: Arc<RwLock<Vec<Regex>>>,
}

impl ClipboardManager {
    /// Construct a manager and load any persisted pins from the DB into
    /// the in-memory history so they're visible to the first search.
    pub fn new(max_entries: usize, db: Arc<Database>) -> Self {
        let history = match load_pins_from_db(&db) {
            Ok(pins) => pins,
            Err(e) => {
                eprintln!("watson: failed to load clipboard pins ({e}); starting empty");
                Vec::new()
            }
        };
        ClipboardManager {
            history: Arc::new(Mutex::new(history)),
            max_entries,
            last_content: Arc::new(Mutex::new(String::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
            monitor_handle: Mutex::new(None),
            db,
            ignore_patterns: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// WAT-303: replace the compiled ignore-pattern list. Applied on the
    /// next monitor tick. Callers compile patterns up-front via
    /// `compile_ignore_patterns` so invalid regexes fail visibly at
    /// settings-load rather than silently at match time.
    pub fn set_ignore_patterns(&self, patterns: Vec<Regex>) {
        *self.ignore_patterns.write().unwrap() = patterns;
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
        let ignore_patterns = Arc::clone(&self.ignore_patterns);
        let max_entries = self.max_entries;

        let handle = thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                if let Some(text) = poll() {
                    record_entry(&history, &last_content, &ignore_patterns, &text, max_entries);
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

    /// Returns the full history with pinned entries first, unpinned after.
    /// Within each group, newest-first (insertion order). Stable sort.
    pub fn get_history(&self) -> Vec<ClipboardEntry> {
        let hist = self.history.lock().unwrap();
        let mut out = hist.clone();
        // b.pinned.cmp(&a.pinned): true > false → pinned first.
        out.sort_by(|a, b| b.pinned.cmp(&a.pinned));
        out
    }

    pub fn search_history(&self, query: &str) -> Vec<ClipboardEntry> {
        let query_lower = query.to_lowercase();
        let hist = self.history.lock().unwrap();
        let mut out: Vec<ClipboardEntry> = hist
            .iter()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();
        out.sort_by(|a, b| b.pinned.cmp(&a.pinned));
        out
    }

    /// WAT-303: clear the unpinned portion of the history. Pinned entries
    /// remain both in memory and on disk — their survival is the whole
    /// point of the pin.
    pub fn clear_history(&self) {
        self.history.lock().unwrap().retain(|e| e.pinned);
    }

    /// WAT-303: mark an entry as pinned. Persists to `clipboard_pins`
    /// so the pin survives app restart. Returns true if a matching
    /// entry was found and pinned.
    pub fn pin(&self, id: &str) -> Result<bool, String> {
        let mut hist = self.history.lock().unwrap();
        let entry = hist.iter_mut().find(|e| e.id == id);
        let Some(entry) = entry else {
            return Ok(false);
        };
        if entry.pinned {
            return Ok(true); // Already pinned — idempotent.
        }
        entry.pinned = true;
        // Snapshot the entry for DB write before we drop the lock.
        let snapshot = entry.clone();
        drop(hist);
        persist_pin(&self.db, &snapshot)?;
        Ok(true)
    }

    /// WAT-303: remove the pin on an entry. The entry stays in memory
    /// (now sorted below pinned entries); it's only unlinked from disk.
    pub fn unpin(&self, id: &str) -> Result<bool, String> {
        let mut hist = self.history.lock().unwrap();
        let entry = hist.iter_mut().find(|e| e.id == id);
        let Some(entry) = entry else {
            return Ok(false);
        };
        if !entry.pinned {
            return Ok(true);
        }
        entry.pinned = false;
        drop(hist);
        remove_pin(&self.db, id)?;
        Ok(true)
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

/// WAT-303: compile a list of pattern strings into regex objects. Returns
/// the successful ones alongside a list of error messages for the bad
/// ones, so the caller can surface them to the user (e.g., via a
/// startup warning) without dropping the good patterns.
pub fn compile_ignore_patterns(patterns: &[String]) -> (Vec<Regex>, Vec<String>) {
    let mut compiled = Vec::new();
    let mut errors = Vec::new();
    for p in patterns {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            continue;
        }
        match Regex::new(trimmed) {
            Ok(re) => compiled.push(re),
            Err(e) => errors.push(format!("'{trimmed}': {e}")),
        }
    }
    (compiled, errors)
}

/// Record a new clipboard text entry into `history` if it differs from the
/// last-seen content AND doesn't match any ignore pattern. Shared between
/// the production and test monitor loops.
fn record_entry(
    history: &Arc<Mutex<Vec<ClipboardEntry>>>,
    last_content: &Arc<Mutex<String>>,
    ignore_patterns: &Arc<RwLock<Vec<Regex>>>,
    text: &str,
    max_entries: usize,
) {
    // WAT-303: drop entries matching the user's privacy filter BEFORE
    // touching history — and don't update `last_content` either, so a
    // filtered value passing through the clipboard a second time is
    // still detected as "new" but still filtered.
    {
        let patterns = ignore_patterns.read().unwrap();
        if patterns.iter().any(|re| re.is_match(text)) {
            return;
        }
    }

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
        pinned: false,
    };

    let mut hist = history.lock().unwrap();
    // Remove duplicate unpinned entries if any (never remove a pinned
    // duplicate — the user explicitly saved it).
    hist.retain(|e| e.pinned || e.content != entry.content);
    // Add to front.
    hist.insert(0, entry);
    // Trim to max size, but never evict pinned entries — they survive
    // the cap even if there are more than `max_entries` total.
    if hist.len() > max_entries {
        let mut kept = 0usize;
        hist.retain(|e| {
            if e.pinned {
                return true;
            }
            kept += 1;
            kept <= max_entries
        });
    }
}

// --- DB helpers (WAT-303) ---

fn persist_pin(db: &Database, entry: &ClipboardEntry) -> Result<(), String> {
    db.execute(
        "INSERT OR REPLACE INTO clipboard_pins (id, content, preview, pinned_at) VALUES (?, ?, ?, ?)",
        &[
            &entry.id,
            &entry.content,
            &entry.preview,
            &entry.timestamp.timestamp(),
        ],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn remove_pin(db: &Database, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM clipboard_pins WHERE id = ?", &[&id])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn load_pins_from_db(db: &Database) -> Result<Vec<ClipboardEntry>, String> {
    db.query_map(
        "SELECT id, content, preview, pinned_at FROM clipboard_pins ORDER BY pinned_at DESC",
        &[],
        |row| {
            let ts: i64 = row.get(3)?;
            Ok(ClipboardEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                preview: row.get(2)?,
                timestamp: DateTime::<Utc>::from_timestamp(ts, 0).unwrap_or_else(Utc::now),
                pinned: true,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    fn manager(max_entries: usize) -> ClipboardManager {
        let db = Arc::new(Database::in_memory().expect("in-memory db"));
        ClipboardManager::new(max_entries, db)
    }

    fn entry(content: &str) -> ClipboardEntry {
        ClipboardEntry {
            id: format!("clip:{content}"),
            content: content.to_string(),
            preview: content.chars().take(100).collect(),
            timestamp: Utc::now(),
            pinned: false,
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
        let mgr = manager(50);
        assert!(mgr.get_history().is_empty());
    }

    // --- get_history ---

    #[test]
    fn get_history_returns_entries_in_stored_order() {
        let mgr = manager(50);
        push_entries(&mgr, &["first", "second", "third"]);
        let hist = mgr.get_history();
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].content, "first");
        assert_eq!(hist[2].content, "third");
    }

    #[test]
    fn get_history_returns_snapshot_not_live_reference() {
        let mgr = manager(50);
        push_entries(&mgr, &["a", "b"]);
        let mut snapshot = mgr.get_history();
        snapshot.clear();
        assert_eq!(mgr.get_history().len(), 2);
    }

    // --- search_history ---

    #[test]
    fn search_history_matches_content_substring() {
        let mgr = manager(50);
        push_entries(&mgr, &["hello world", "foobar", "hello again"]);
        let results = mgr.search_history("hello");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_history_is_case_insensitive() {
        let mgr = manager(50);
        push_entries(&mgr, &["Hello World"]);
        assert_eq!(mgr.search_history("hello").len(), 1);
        assert_eq!(mgr.search_history("WORLD").len(), 1);
    }

    #[test]
    fn search_history_returns_empty_on_no_match() {
        let mgr = manager(50);
        push_entries(&mgr, &["foo", "bar"]);
        assert!(mgr.search_history("xylophone").is_empty());
    }

    #[test]
    fn search_history_empty_query_matches_everything() {
        let mgr = manager(50);
        push_entries(&mgr, &["a", "b", "c"]);
        assert_eq!(mgr.search_history("").len(), 3);
    }

    // --- clear_history ---

    #[test]
    fn clear_history_empties_unpinned_state() {
        let mgr = manager(50);
        push_entries(&mgr, &["a", "b", "c"]);
        mgr.clear_history();
        assert!(mgr.get_history().is_empty());
    }

    #[test]
    fn clear_history_is_ok_on_empty_manager() {
        let mgr = manager(50);
        mgr.clear_history();
        assert!(mgr.get_history().is_empty());
    }

    // --- WAT-303 pin/unpin ---

    #[test]
    fn pin_marks_entry_as_pinned_and_returns_true() {
        let mgr = manager(50);
        push_entries(&mgr, &["ephemeral", "keep-me"]);
        let id = mgr.get_history()[1].id.clone();
        assert!(mgr.pin(&id).unwrap());
        let hist = mgr.get_history();
        assert!(hist.iter().find(|e| e.id == id).unwrap().pinned);
    }

    #[test]
    fn pin_unknown_id_returns_false() {
        let mgr = manager(50);
        assert!(!mgr.pin("clip:does-not-exist").unwrap());
    }

    #[test]
    fn pin_is_idempotent() {
        let mgr = manager(50);
        push_entries(&mgr, &["content"]);
        let id = mgr.get_history()[0].id.clone();
        assert!(mgr.pin(&id).unwrap());
        // Second pin: still true, no error, no duplicate row.
        assert!(mgr.pin(&id).unwrap());
    }

    #[test]
    fn unpin_clears_pinned_flag() {
        let mgr = manager(50);
        push_entries(&mgr, &["content"]);
        let id = mgr.get_history()[0].id.clone();
        mgr.pin(&id).unwrap();
        mgr.unpin(&id).unwrap();
        assert!(!mgr.get_history()[0].pinned);
    }

    // --- WAT-303 get_history order ---

    #[test]
    fn get_history_sorts_pinned_entries_first() {
        // Three entries; pin only the last. After get_history, pinned
        // should come first while newest-first order holds within
        // each group.
        let mgr = manager(50);
        push_entries(&mgr, &["a", "b", "c"]);
        let c_id = mgr.get_history()[2].id.clone();
        mgr.pin(&c_id).unwrap();
        let sorted = mgr.get_history();
        assert_eq!(sorted[0].content, "c", "pinned 'c' should come first");
        assert_eq!(sorted[1].content, "a");
        assert_eq!(sorted[2].content, "b");
    }

    // --- WAT-303 clear_history preserves pins ---

    #[test]
    fn clear_history_preserves_pinned_entries() {
        let mgr = manager(50);
        push_entries(&mgr, &["ephemeral", "keep-me"]);
        let id = mgr.get_history()[1].id.clone();
        mgr.pin(&id).unwrap();

        mgr.clear_history();

        let hist = mgr.get_history();
        assert_eq!(hist.len(), 1, "pinned entry must survive clear");
        assert_eq!(hist[0].content, "keep-me");
        assert!(hist[0].pinned);
    }

    // --- WAT-303 persistence ---

    #[test]
    fn pins_persist_across_manager_recreation() {
        // Bake a pin into a shared in-memory DB, then drop and recreate
        // the manager. The pin should reappear in the new instance's
        // history.
        let db = Arc::new(Database::in_memory().unwrap());
        {
            let mgr = ClipboardManager::new(50, Arc::clone(&db));
            push_entries(&mgr, &["surviving-pin"]);
            let id = mgr.get_history()[0].id.clone();
            mgr.pin(&id).unwrap();
        }

        let mgr2 = ClipboardManager::new(50, Arc::clone(&db));
        let hist = mgr2.get_history();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].content, "surviving-pin");
        assert!(hist[0].pinned);
    }

    #[test]
    fn unpinning_removes_the_db_row() {
        let db = Arc::new(Database::in_memory().unwrap());
        let mgr = ClipboardManager::new(50, Arc::clone(&db));
        push_entries(&mgr, &["content"]);
        let id = mgr.get_history()[0].id.clone();
        mgr.pin(&id).unwrap();
        mgr.unpin(&id).unwrap();

        // Fresh manager sees nothing — the pin was removed from DB.
        drop(mgr);
        let mgr2 = ClipboardManager::new(50, Arc::clone(&db));
        assert!(mgr2.get_history().is_empty());
    }

    // --- WAT-303 ignore patterns ---

    #[test]
    fn compile_ignore_patterns_returns_empty_for_no_input() {
        let (compiled, errors) = compile_ignore_patterns(&[]);
        assert!(compiled.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn compile_ignore_patterns_skips_blank_lines() {
        let input: Vec<String> = vec!["  ".into(), "".into(), "valid".into()];
        let (compiled, errors) = compile_ignore_patterns(&input);
        assert_eq!(compiled.len(), 1);
        assert!(errors.is_empty());
    }

    #[test]
    fn compile_ignore_patterns_reports_invalid_regex_without_dropping_good_ones() {
        let input: Vec<String> = vec![r"\d+".into(), r"[invalid".into(), r"^\s*$".into()];
        let (compiled, errors) = compile_ignore_patterns(&input);
        assert_eq!(compiled.len(), 2, "good patterns should still compile");
        assert_eq!(errors.len(), 1, "one invalid pattern reported");
        assert!(errors[0].contains("[invalid"));
    }

    #[test]
    fn ignore_pattern_prevents_recording() {
        let mgr = manager(50);
        let re = Regex::new(r"^secret-[a-f0-9]+$").unwrap();
        mgr.set_ignore_patterns(vec![re]);

        // Short-circuit approach: skip the monitor entirely and call
        // the internal `record_entry` directly. This keeps the test
        // deterministic (no dependency on how many polls fire per
        // wallclock window) while still exercising the production
        // filter logic end-to-end.
        let patterns = Arc::clone(&mgr.ignore_patterns);
        record_entry(
            &mgr.history,
            &mgr.last_content,
            &patterns,
            "secret-abc123",
            50,
        );
        record_entry(
            &mgr.history,
            &mgr.last_content,
            &patterns,
            "hello",
            50,
        );

        let hist = mgr.get_history();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].content, "hello");
    }

    // --- WAT-106: cooperative shutdown (preserved from earlier PR) ---

    #[test]
    fn shutdown_is_idempotent_when_never_started() {
        let mgr = manager(50);
        mgr.shutdown();
        mgr.shutdown();
    }

    #[test]
    fn shutdown_stops_a_running_monitor_quickly() {
        let mgr = manager(50);
        let poll_count = Arc::new(AtomicUsize::new(0));
        let pc = Arc::clone(&poll_count);
        mgr.start_monitoring_with_poller(move || {
            pc.fetch_add(1, Ordering::Relaxed);
            None
        });

        thread::sleep(Duration::from_millis(150));
        assert!(poll_count.load(Ordering::Relaxed) >= 1);

        let start = Instant::now();
        mgr.shutdown();
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(500), "shutdown took {elapsed:?}");
    }

    #[test]
    fn drop_joins_the_monitor_thread() {
        let poll_count = Arc::new(AtomicUsize::new(0));
        let pc = Arc::clone(&poll_count);
        {
            let mgr = manager(50);
            mgr.start_monitoring_with_poller(move || {
                pc.fetch_add(1, Ordering::Relaxed);
                None
            });
            thread::sleep(Duration::from_millis(100));
        }
        let snapshot = poll_count.load(Ordering::Relaxed);
        thread::sleep(Duration::from_millis(100));
        let after = poll_count.load(Ordering::Relaxed);
        assert_eq!(snapshot, after);
    }

    #[test]
    fn monitor_records_entries_via_injected_poller() {
        let mgr = manager(50);
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
