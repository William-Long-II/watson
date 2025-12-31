# Watson v2: Notes & File Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add persistent notes, ephemeral scratchpad, and file search to Watson's unified search.

**Architecture:** Extend existing search system with new result types (Note, File, Scratchpad). Notes stored in SQLite + markdown files. Files indexed in background with filesystem watcher. All sources ranked together in unified search.

**Tech Stack:** Rust (Tauri backend), React/TypeScript (frontend), SQLite (storage), notify crate (file watching)

---

## Phase 1: Database Schema Updates

### Task 1.1: Add New Tables to Schema

**Files:**
- Modify: `src-tauri/src/db/schema.rs`

**Step 1: Update the schema constant**

Add these tables after the existing `icons` table:

```rust
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    icon_cache_path TEXT,
    launch_count INTEGER DEFAULT 0,
    last_launched INTEGER,
    platform TEXT NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS search_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query TEXT NOT NULL,
    selected_item_id TEXT,
    selected_item_type TEXT,
    timestamp INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS icons (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    cache_path TEXT NOT NULL,
    hash TEXT NOT NULL,
    cached_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS note_tags (
    note_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (note_id, tag),
    FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    extension TEXT,
    size_bytes INTEGER,
    modified_at INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS scratchpad (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    content TEXT NOT NULL DEFAULT '',
    modified_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_apps_name ON apps(name);
CREATE INDEX IF NOT EXISTS idx_history_query ON search_history(query);
CREATE INDEX IF NOT EXISTS idx_history_timestamp ON search_history(timestamp);
CREATE INDEX IF NOT EXISTS idx_notes_title ON notes(title);
CREATE INDEX IF NOT EXISTS idx_notes_modified ON notes(modified_at);
CREATE INDEX IF NOT EXISTS idx_note_tags_tag ON note_tags(tag);
CREATE INDEX IF NOT EXISTS idx_files_name ON files(name);
CREATE INDEX IF NOT EXISTS idx_files_extension ON files(extension);
CREATE INDEX IF NOT EXISTS idx_files_modified ON files(modified_at);
"#;
```

**Step 2: Verify the app compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add src-tauri/src/db/schema.rs
git commit -m "feat(db): add schema for notes, files, and scratchpad"
```

---

## Phase 2: Scratchpad Feature (Backend)

### Task 2.1: Create Scratchpad Module

**Files:**
- Create: `src-tauri/src/scratchpad.rs`

**Step 1: Create the scratchpad module**

```rust
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
```

**Step 2: Add module to lib.rs**

In `src-tauri/src/lib.rs`, add after line 6:

```rust
mod scratchpad;
```

And add import:

```rust
use scratchpad::ScratchpadManager;
```

**Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 4: Run tests**

Run: `cd src-tauri && cargo test scratchpad`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src-tauri/src/scratchpad.rs src-tauri/src/lib.rs
git commit -m "feat: add scratchpad backend module"
```

---

### Task 2.2: Add Scratchpad Tauri Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add ScratchpadManager to AppState**

Update the AppState struct (around line 24):

```rust
struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
    clipboard: ClipboardManager,
    scratchpad: ScratchpadManager,
}
```

**Step 2: Add Tauri commands**

Add after the `copy_to_clipboard` command (around line 243):

```rust
#[tauri::command]
fn get_scratchpad(state: State<AppState>) -> Result<scratchpad::Scratchpad, String> {
    state.scratchpad.get()
}

#[tauri::command]
fn set_scratchpad(content: String, state: State<AppState>) -> Result<(), String> {
    state.scratchpad.set(&content)
}

#[tauri::command]
fn clear_scratchpad(state: State<AppState>) -> Result<(), String> {
    state.scratchpad.clear()
}
```

**Step 3: Initialize ScratchpadManager in run()**

In the `run()` function, after clipboard initialization (around line 253):

```rust
// Initialize scratchpad manager
let scratchpad = ScratchpadManager::new(Arc::clone(&Arc::new(db)));
```

Wait, we need to restructure slightly. Update the db initialization:

```rust
let db = Arc::new(Database::new().expect("Failed to initialize database"));
let scratchpad = ScratchpadManager::new(Arc::clone(&db));
```

And update AppState initialization:

```rust
.manage(AppState {
    db: Arc::clone(&db),
    search_engine: SearchEngine::new(),
    indexed_apps: RwLock::new(indexed_apps),
    settings: RwLock::new(settings),
    clipboard,
    scratchpad,
})
```

**Step 4: Register commands in invoke_handler**

Add to the invoke_handler list:

```rust
.invoke_handler(tauri::generate_handler![
    get_settings,
    save_settings_cmd,
    reindex_apps,
    search,
    execute_action,
    hide_window,
    show_window,
    resize_window,
    get_clipboard_history,
    search_clipboard,
    clear_clipboard_history,
    copy_to_clipboard,
    get_scratchpad,
    set_scratchpad,
    clear_scratchpad
])
```

**Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add scratchpad Tauri commands"
```

---

## Phase 3: Scratchpad Feature (Frontend)

### Task 3.1: Add Scratchpad Types

**Files:**
- Modify: `src/types.ts`

**Step 1: Add Scratchpad type**

Add at the end of the file:

```typescript
export interface Scratchpad {
  content: string;
  modified_at: number;
}
```

**Step 2: Commit**

```bash
git add src/types.ts
git commit -m "feat: add Scratchpad type"
```

---

### Task 3.2: Add Scratchpad to Store

**Files:**
- Modify: `src/stores/app.ts`

**Step 1: Read current store**

First, read the current store to understand its structure.

**Step 2: Add scratchpad state and actions**

Add to the store interface:

```typescript
// In the store state
scratchpad: string;
scratchpadVisible: boolean;

// In the store actions
loadScratchpad: () => Promise<void>;
saveScratchpad: (content: string) => Promise<void>;
clearScratchpad: () => Promise<void>;
setShowScratchpad: (show: boolean) => void;
```

Add implementations:

```typescript
scratchpad: '',
scratchpadVisible: false,

loadScratchpad: async () => {
  try {
    const pad = await invoke<Scratchpad>('get_scratchpad');
    set({ scratchpad: pad.content });
  } catch (e) {
    console.error('Failed to load scratchpad:', e);
  }
},

saveScratchpad: async (content: string) => {
  try {
    await invoke('set_scratchpad', { content });
    set({ scratchpad: content });
  } catch (e) {
    console.error('Failed to save scratchpad:', e);
  }
},

clearScratchpad: async () => {
  try {
    await invoke('clear_scratchpad');
    set({ scratchpad: '' });
  } catch (e) {
    console.error('Failed to clear scratchpad:', e);
  }
},

setShowScratchpad: (show: boolean) => set({ scratchpadVisible: show }),
```

**Step 3: Add Scratchpad import**

```typescript
import type { Scratchpad } from '../types';
```

**Step 4: Commit**

```bash
git add src/stores/app.ts
git commit -m "feat: add scratchpad state to store"
```

---

### Task 3.3: Create Scratchpad Component

**Files:**
- Create: `src/components/Scratchpad.tsx`

**Step 1: Create the component**

```tsx
import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';

export function Scratchpad() {
  const { scratchpad, saveScratchpad, clearScratchpad, setShowScratchpad, loadScratchpad } = useAppStore();
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    loadScratchpad();
  }, []);

  useEffect(() => {
    // Focus textarea when scratchpad opens
    textareaRef.current?.focus();
  }, []);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    saveScratchpad(e.target.value);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setShowScratchpad(false);
    }
  };

  return (
    <div className="p-4 border-t border-[var(--border)]">
      <div className="flex justify-between items-center mb-3">
        <h3 className="font-semibold flex items-center gap-2">
          <span className="text-lg">📋</span>
          Scratchpad
        </h3>
        <div className="flex gap-2">
          <button
            onClick={() => clearScratchpad()}
            className="px-2 py-1 text-xs text-gray-500 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
          >
            Clear
          </button>
          <button
            onClick={() => setShowScratchpad(false)}
            className="text-gray-400 hover:text-gray-600"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
      <textarea
        ref={textareaRef}
        value={scratchpad}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        placeholder="Jot something down..."
        className="w-full h-48 p-3 text-sm bg-[var(--input-bg)] border border-[var(--border)] rounded-lg resize-none outline-none focus:ring-1 focus:ring-blue-500"
      />
      <p className="text-xs text-gray-400 mt-2">Press Escape to close. Auto-saves as you type.</p>
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add src/components/Scratchpad.tsx
git commit -m "feat: add Scratchpad component"
```

---

### Task 3.4: Integrate Scratchpad into App

**Files:**
- Modify: `src/App.tsx`

**Step 1: Import Scratchpad component**

Add import at top:

```typescript
import { Scratchpad } from './components/Scratchpad';
```

**Step 2: Add scratchpadVisible to store destructuring**

Update the App component's store usage:

```typescript
const { loadSettings, reindexApps, settings, showSettings, setShowSettings, resizeWindow, scratchpadVisible } = useAppStore();
```

**Step 3: Add Scratchpad to render**

Update the return statement to show Scratchpad when visible:

```tsx
return (
  <div className="bg-[var(--background)] text-[var(--foreground)] rounded-xl overflow-hidden border border-[var(--border)] shadow-2xl">
    {/* Header - draggable */}
    <div
      data-tauri-drag-region
      onMouseDown={async (e) => {
        if ((e.target as HTMLElement).closest('button')) return;
        e.preventDefault();
        try {
          await getCurrentWindow().startDragging();
        } catch (err) {
          console.error('Failed to start dragging:', err);
        }
      }}
      className="flex items-center justify-between px-4 py-3 border-b border-[var(--border)] cursor-move select-none"
    >
      <div className="flex items-center gap-2 pointer-events-none">
        <WatsonLogo />
        <span className="text-lg font-semibold">Watson</span>
      </div>
      <SettingsIcon onClick={() => setShowSettings(!showSettings)} />
    </div>

    <SearchBar />

    {scratchpadVisible ? (
      <Scratchpad />
    ) : showSettings ? (
      <SettingsPanel onClose={() => setShowSettings(false)} />
    ) : (
      <ResultsList />
    )}
  </div>
);
```

**Step 4: Verify it compiles**

Run: `npm run build`
Expected: Builds without errors

**Step 5: Commit**

```bash
git add src/App.tsx
git commit -m "feat: integrate Scratchpad into main App"
```

---

### Task 3.5: Add Scratchpad Trigger to Search

**Files:**
- Modify: `src/stores/app.ts`

**Step 1: Update the search/query handling**

Add logic to detect scratchpad triggers. In the store, add a function to handle special queries:

```typescript
handleQuery: (query: string) => {
  const { setShowScratchpad, setShowSettings } = get();

  // Check for scratchpad triggers
  if (query === 'scratch' || query === '`' || query === 's') {
    setShowScratchpad(true);
    return true; // Indicates query was handled
  }

  return false; // Query not handled, continue normal search
},
```

**Step 2: Commit**

```bash
git add src/stores/app.ts
git commit -m "feat: add scratchpad trigger detection"
```

---

### Task 3.6: Wire Up Scratchpad Trigger in SearchBar

**Files:**
- Modify: `src/components/SearchBar.tsx`

**Step 1: Read current SearchBar to understand structure**

**Step 2: Add trigger handling**

Update the search input's onChange or onKeyDown to check for scratchpad triggers when query is empty and user types trigger key.

This depends on the current SearchBar implementation. The key logic:

```typescript
// On keydown, if search is empty and key is 's' or '`'
if (query === '' && (e.key === 's' || e.key === '`')) {
  e.preventDefault();
  setShowScratchpad(true);
}
```

**Step 3: Commit**

```bash
git add src/components/SearchBar.tsx
git commit -m "feat: add scratchpad keyboard trigger"
```

---

## Phase 4: Notes Feature (Backend)

### Task 4.1: Create Notes Module Structure

**Files:**
- Create: `src-tauri/src/notes/mod.rs`
- Create: `src-tauri/src/notes/storage.rs`
- Create: `src-tauri/src/notes/tags.rs`

**Step 1: Create mod.rs**

```rust
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
        let tags = tags::extract_tags(content);

        // Insert into database
        self.db
            .execute(
                "INSERT INTO notes (id, title, content, created_at, modified_at) VALUES (?, ?, ?, ?, ?)",
                &[&id, &title, &content, &now, &now],
            )
            .map_err(|e| e.to_string())?;

        // Insert tags
        for tag in &tags {
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
            tags,
            created_at: now,
            modified_at: now,
        })
    }

    pub fn update(&self, id: &str, title: &str, content: &str) -> Result<Note, String> {
        let now = Utc::now().timestamp();
        let tags = tags::extract_tags(content);

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
        for tag in &tags {
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
            tags,
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
            let tags = self.get_tags(&id)?;
            Ok(Some(Note {
                id,
                title,
                content,
                tags,
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
            let tags = self.get_tags(&id)?;
            notes.push(Note {
                id,
                title,
                content,
                tags,
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
            let tags = self.get_tags(&id)?;
            notes.push(Note {
                id,
                title,
                content,
                tags,
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
```

**Step 2: Create tags.rs**

```rust
/// Extract hashtags from content
pub fn extract_tags(content: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for word in content.split_whitespace() {
        if word.starts_with('#') && word.len() > 1 {
            let tag = word
                .trim_start_matches('#')
                .trim_end_matches(|c: char| !c.is_alphanumeric());
            if !tag.is_empty() {
                tags.push(tag.to_lowercase());
            }
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tags() {
        assert_eq!(extract_tags("hello #world"), vec!["world"]);
        assert_eq!(extract_tags("#one #two #three"), vec!["one", "three", "two"]);
        assert_eq!(extract_tags("no tags here"), Vec::<String>::new());
        assert_eq!(extract_tags("#Work meeting notes"), vec!["work"]);
        assert_eq!(extract_tags("# not a tag"), Vec::<String>::new());
    }
}
```

**Step 3: Create storage.rs**

```rust
use std::path::Path;

pub fn write_note_file(
    storage_path: &Path,
    id: &str,
    title: &str,
    content: &str,
) -> Result<(), String> {
    let safe_title = sanitize_filename(title);
    let filename = format!("{}-{}.md", id.replace("note:", ""), safe_title);
    let path = storage_path.join(&filename);

    let file_content = format!("# {}\n\n{}", title, content);
    std::fs::write(&path, file_content).map_err(|e| e.to_string())
}

pub fn delete_note_file(storage_path: &Path, id: &str) -> Result<(), String> {
    // Find and delete the file matching this id
    if let Ok(entries) = std::fs::read_dir(storage_path) {
        let prefix = id.replace("note:", "");
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&prefix) {
                    std::fs::remove_file(entry.path()).ok();
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .chars()
        .take(50)
        .collect()
}
```

**Step 4: Add module to lib.rs**

Add after line 6:

```rust
mod notes;
```

**Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 6: Run tests**

Run: `cd src-tauri && cargo test tags`
Expected: All tests pass

**Step 7: Commit**

```bash
git add src-tauri/src/notes/
git commit -m "feat: add notes backend module with CRUD and file storage"
```

---

### Task 4.2: Add Notes Tauri Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add NotesManager to AppState**

```rust
use notes::NotesManager;

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
    clipboard: ClipboardManager,
    scratchpad: ScratchpadManager,
    notes: NotesManager,
}
```

**Step 2: Add Tauri commands**

```rust
#[tauri::command]
fn create_note(title: String, content: String, state: State<AppState>) -> Result<notes::Note, String> {
    state.notes.create(&title, &content)
}

#[tauri::command]
fn update_note(id: String, title: String, content: String, state: State<AppState>) -> Result<notes::Note, String> {
    state.notes.update(&id, &title, &content)
}

#[tauri::command]
fn delete_note(id: String, state: State<AppState>) -> Result<(), String> {
    state.notes.delete(&id)
}

#[tauri::command]
fn get_note(id: String, state: State<AppState>) -> Result<Option<notes::Note>, String> {
    state.notes.get(&id)
}

#[tauri::command]
fn search_notes(query: String, state: State<AppState>) -> Result<Vec<notes::Note>, String> {
    state.notes.search(&query)
}

#[tauri::command]
fn get_recent_notes(limit: usize, state: State<AppState>) -> Result<Vec<notes::Note>, String> {
    state.notes.get_recent(limit)
}
```

**Step 3: Initialize NotesManager in run()**

Get notes storage path from settings or use default:

```rust
let notes_path = directories::ProjectDirs::from("com", "watson", "Watson")
    .map(|dirs| dirs.data_dir().join("notes"))
    .unwrap_or_else(|| std::path::PathBuf::from("./notes"));
let notes = NotesManager::new(Arc::clone(&db), notes_path);
```

Update AppState:

```rust
.manage(AppState {
    db: Arc::clone(&db),
    search_engine: SearchEngine::new(),
    indexed_apps: RwLock::new(indexed_apps),
    settings: RwLock::new(settings),
    clipboard,
    scratchpad,
    notes,
})
```

**Step 4: Register commands**

Add to invoke_handler:

```rust
create_note,
update_note,
delete_note,
get_note,
search_notes,
get_recent_notes,
```

**Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add notes Tauri commands"
```

---

## Phase 5: File Search Feature (Backend)

### Task 5.1: Add File Search Dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add notify crate for file watching**

Add to dependencies:

```toml
notify = "6.1"
walkdir = "2.4"
```

**Step 2: Verify dependencies**

Run: `cd src-tauri && cargo check`
Expected: Dependencies download and compile

**Step 3: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore: add notify and walkdir dependencies for file search"
```

---

### Task 5.2: Create File Search Module

**Files:**
- Create: `src-tauri/src/files/mod.rs`
- Create: `src-tauri/src/files/indexer.rs`

**Step 1: Create mod.rs**

```rust
pub mod indexer;

use crate::db::Database;
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

    pub fn search(&self, query: &str) -> Result<Vec<FileEntry>, String> {
        let pattern = format!("%{}%", query);
        self.db
            .query_map(
                "SELECT id, name, path, extension, size_bytes, modified_at FROM files
                 WHERE name LIKE ? OR path LIKE ?
                 ORDER BY modified_at DESC LIMIT 50",
                &[&pattern, &pattern],
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

    pub fn search_by_extension(&self, ext: &str) -> Result<Vec<FileEntry>, String> {
        let ext_lower = ext.to_lowercase().trim_start_matches('.').to_string();
        self.db
            .query_map(
                "SELECT id, name, path, extension, size_bytes, modified_at FROM files
                 WHERE extension = ?
                 ORDER BY modified_at DESC LIMIT 50",
                &[&ext_lower],
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
                    &chrono::Utc::now().timestamp(),
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
        self.db
            .execute("DELETE FROM files", &[])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
```

**Step 2: Create indexer.rs**

```rust
use super::{FileEntry, FileSearchManager};
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

pub struct FileIndexer {
    manager: Arc<FileSearchManager>,
    indexed_paths: Vec<String>,
    excluded_patterns: Vec<String>,
    max_depth: usize,
}

impl FileIndexer {
    pub fn new(
        manager: Arc<FileSearchManager>,
        indexed_paths: Vec<String>,
        excluded_patterns: Vec<String>,
        max_depth: usize,
    ) -> Self {
        FileIndexer {
            manager,
            indexed_paths,
            excluded_patterns,
            max_depth,
        }
    }

    pub fn index_all(&self) -> usize {
        let mut count = 0;
        for path_str in &self.indexed_paths {
            let path = expand_path(path_str);
            if path.exists() && path.is_dir() {
                count += self.index_directory(&path);
            }
        }
        count
    }

    fn index_directory(&self, dir: &Path) -> usize {
        let mut count = 0;
        let walker = WalkDir::new(dir)
            .max_depth(self.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.is_excluded(e.path()));

        for entry in walker.flatten() {
            if entry.file_type().is_file() {
                if let Some(file_entry) = self.create_file_entry(entry.path()) {
                    if self.manager.insert(&file_entry).is_ok() {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.excluded_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }
        false
    }

    fn create_file_entry(&self, path: &Path) -> Option<FileEntry> {
        let metadata = path.metadata().ok()?;
        let name = path.file_name()?.to_string_lossy().to_string();
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|| Utc::now().timestamp());

        Some(FileEntry {
            id: format!("file:{}", path.to_string_lossy().replace(['/', '\\'], "-")),
            name,
            path: path.to_string_lossy().to_string(),
            extension,
            size_bytes: Some(metadata.len() as i64),
            modified_at,
        })
    }
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
}
```

**Step 3: Add module to lib.rs**

```rust
mod files;
```

**Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add src-tauri/src/files/
git commit -m "feat: add file search backend module with indexer"
```

---

### Task 5.3: Add File Settings to Config

**Files:**
- Modify: `src-tauri/src/config/settings.rs`

**Step 1: Add FileSearchSettings struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSearchSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_indexed_paths")]
    pub indexed_paths: Vec<String>,
    #[serde(default = "default_excluded_patterns")]
    pub excluded_patterns: Vec<String>,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

fn default_indexed_paths() -> Vec<String> {
    let mut paths = vec![];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join("Downloads").to_string_lossy().to_string());
        paths.push(home.join("Documents").to_string_lossy().to_string());
    }
    paths
}

fn default_excluded_patterns() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        ".git".to_string(),
        "target".to_string(),
        ".cache".to_string(),
    ]
}

fn default_max_depth() -> usize {
    10
}
```

**Step 2: Add to Settings struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub general: GeneralSettings,
    pub activation: ActivationSettings,
    pub search: SearchSettings,
    pub theme: ThemeSettings,
    #[serde(default)]
    pub web_searches: Vec<WebSearch>,
    #[serde(default)]
    pub file_search: FileSearchSettings,
}
```

**Step 3: Update Default impl**

```rust
impl Default for Settings {
    fn default() -> Self {
        Settings {
            // ... existing fields ...
            file_search: FileSearchSettings {
                enabled: true,
                indexed_paths: default_indexed_paths(),
                excluded_patterns: default_excluded_patterns(),
                max_depth: 10,
            },
        }
    }
}
```

**Step 4: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 5: Commit**

```bash
git add src-tauri/src/config/settings.rs
git commit -m "feat: add file search settings to config"
```

---

### Task 5.4: Add File Search Tauri Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add FileSearchManager to AppState**

```rust
use files::FileSearchManager;

struct AppState {
    // ... existing fields ...
    file_search: Arc<FileSearchManager>,
}
```

**Step 2: Add Tauri commands**

```rust
#[tauri::command]
fn search_files(query: String, state: State<AppState>) -> Result<Vec<files::FileEntry>, String> {
    // Check if query is extension filter
    if query.starts_with('.') {
        state.file_search.search_by_extension(&query)
    } else {
        state.file_search.search(&query)
    }
}

#[tauri::command]
fn reindex_files(state: State<AppState>) -> Result<usize, String> {
    let settings = state.settings.read().unwrap();
    let indexer = files::indexer::FileIndexer::new(
        Arc::clone(&state.file_search),
        settings.file_search.indexed_paths.clone(),
        settings.file_search.excluded_patterns.clone(),
        settings.file_search.max_depth,
    );

    // Clear and reindex
    state.file_search.clear_all()?;
    Ok(indexer.index_all())
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    open::that(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        // Open containing folder
        if let Some(parent) = std::path::Path::new(&path).parent() {
            open::that(parent).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
```

**Step 3: Initialize FileSearchManager**

```rust
let file_search = Arc::new(FileSearchManager::new(Arc::clone(&db)));

// Background index on startup
if settings.file_search.enabled {
    let fs_clone = Arc::clone(&file_search);
    let paths = settings.file_search.indexed_paths.clone();
    let excluded = settings.file_search.excluded_patterns.clone();
    let depth = settings.file_search.max_depth;
    std::thread::spawn(move || {
        let indexer = files::indexer::FileIndexer::new(fs_clone, paths, excluded, depth);
        let count = indexer.index_all();
        println!("Indexed {} files", count);
    });
}
```

**Step 4: Update AppState and invoke_handler**

Add `file_search: Arc::clone(&file_search)` to AppState.

Add to invoke_handler:
```rust
search_files,
reindex_files,
open_file,
reveal_file,
```

**Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add file search Tauri commands"
```

---

## Phase 6: Frontend - Notes & Files Types and Store

### Task 6.1: Add Frontend Types

**Files:**
- Modify: `src/types.ts`

**Step 1: Add types**

```typescript
export interface Note {
  id: string;
  title: string;
  content: string;
  tags: string[];
  created_at: number;
  modified_at: number;
}

export interface FileEntry {
  id: string;
  name: string;
  path: string;
  extension: string | null;
  size_bytes: number | null;
  modified_at: number;
}

export interface FileSearchSettings {
  enabled: boolean;
  indexed_paths: string[];
  excluded_patterns: string[];
  max_depth: number;
}
```

**Step 2: Update result_type union**

```typescript
export interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  result_type: 'application' | 'web_search' | 'system_command' | 'clipboard' | 'note' | 'file';
  score: number;
  action: SearchAction;
}

export type SearchAction =
  | { type: 'launch_app'; path: string }
  | { type: 'open_url'; url: string }
  | { type: 'run_command'; command: string }
  | { type: 'copy_clipboard'; content: string }
  | { type: 'open_note'; id: string }
  | { type: 'open_file'; path: string }
  | { type: 'reveal_file'; path: string };
```

**Step 3: Update Settings interface**

```typescript
export interface Settings {
  general: GeneralSettings;
  activation: ActivationSettings;
  search: SearchSettings;
  theme: ThemeSettings;
  web_searches: WebSearch[];
  file_search: FileSearchSettings;
}
```

**Step 4: Commit**

```bash
git add src/types.ts
git commit -m "feat: add Note, FileEntry types and update SearchResult"
```

---

### Task 6.2: Add Notes and Files to Store

**Files:**
- Modify: `src/stores/app.ts`

**Step 1: Add state and actions for notes**

```typescript
// State
editingNote: Note | null;
noteEditorVisible: boolean;

// Actions
createNote: (title: string, content: string) => Promise<Note>;
updateNote: (id: string, title: string, content: string) => Promise<Note>;
deleteNote: (id: string) => Promise<void>;
searchNotes: (query: string) => Promise<Note[]>;
getRecentNotes: (limit: number) => Promise<Note[]>;
setEditingNote: (note: Note | null) => void;
setNoteEditorVisible: (visible: boolean) => void;
```

**Step 2: Add state and actions for files**

```typescript
// Actions
searchFiles: (query: string) => Promise<FileEntry[]>;
reindexFiles: () => Promise<number>;
openFile: (path: string) => Promise<void>;
revealFile: (path: string) => Promise<void>;
```

**Step 3: Implement the actions**

```typescript
editingNote: null,
noteEditorVisible: false,

createNote: async (title, content) => {
  return await invoke<Note>('create_note', { title, content });
},

updateNote: async (id, title, content) => {
  return await invoke<Note>('update_note', { id, title, content });
},

deleteNote: async (id) => {
  await invoke('delete_note', { id });
},

searchNotes: async (query) => {
  return await invoke<Note[]>('search_notes', { query });
},

getRecentNotes: async (limit) => {
  return await invoke<Note[]>('get_recent_notes', { limit });
},

setEditingNote: (note) => set({ editingNote: note, noteEditorVisible: note !== null }),

setNoteEditorVisible: (visible) => set({ noteEditorVisible: visible }),

searchFiles: async (query) => {
  return await invoke<FileEntry[]>('search_files', { query });
},

reindexFiles: async () => {
  return await invoke<number>('reindex_files');
},

openFile: async (path) => {
  await invoke('open_file', { path });
},

revealFile: async (path) => {
  await invoke('reveal_file', { path });
},
```

**Step 4: Commit**

```bash
git add src/stores/app.ts
git commit -m "feat: add notes and files actions to store"
```

---

## Phase 7: Frontend - Note Editor Component

### Task 7.1: Create NoteEditor Component

**Files:**
- Create: `src/components/NoteEditor.tsx`

**Step 1: Create the component**

```tsx
import { useState, useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';
import type { Note } from '../types';

interface NoteEditorProps {
  note?: Note;
  initialTitle?: string;
  onClose: () => void;
}

export function NoteEditor({ note, initialTitle, onClose }: NoteEditorProps) {
  const { createNote, updateNote, deleteNote } = useAppStore();
  const [title, setTitle] = useState(note?.title || initialTitle || '');
  const [content, setContent] = useState(note?.content || '');
  const [saving, setSaving] = useState(false);
  const contentRef = useRef<HTMLTextAreaElement>(null);
  const saveTimeoutRef = useRef<number>();

  useEffect(() => {
    contentRef.current?.focus();
  }, []);

  // Auto-save on content change (debounced)
  useEffect(() => {
    if (note && (title !== note.title || content !== note.content)) {
      clearTimeout(saveTimeoutRef.current);
      saveTimeoutRef.current = window.setTimeout(async () => {
        setSaving(true);
        await updateNote(note.id, title, content);
        setSaving(false);
      }, 500);
    }
    return () => clearTimeout(saveTimeoutRef.current);
  }, [title, content, note, updateNote]);

  const handleSave = async () => {
    if (!title.trim()) return;
    setSaving(true);
    if (note) {
      await updateNote(note.id, title, content);
    } else {
      await createNote(title, content);
    }
    setSaving(false);
    onClose();
  };

  const handleDelete = async () => {
    if (note && confirm('Delete this note?')) {
      await deleteNote(note.id);
      onClose();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
    }
    if (e.key === 's' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSave();
    }
  };

  return (
    <div className="p-4 border-t border-[var(--border)]" onKeyDown={handleKeyDown}>
      <div className="flex justify-between items-center mb-3">
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Note title..."
          className="text-lg font-semibold bg-transparent outline-none flex-1"
        />
        <div className="flex items-center gap-2">
          {saving && <span className="text-xs text-gray-400">Saving...</span>}
          {note && (
            <button
              onClick={handleDelete}
              className="px-2 py-1 text-xs text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
            >
              Delete
            </button>
          )}
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-gray-600"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
      <textarea
        ref={contentRef}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        placeholder="Write your note... Use #hashtags to tag."
        className="w-full h-48 p-3 text-sm bg-[var(--input-bg)] border border-[var(--border)] rounded-lg resize-none outline-none focus:ring-1 focus:ring-blue-500"
      />
      <div className="flex justify-between items-center mt-2">
        <p className="text-xs text-gray-400">
          {note ? 'Auto-saves as you type' : 'Press Cmd/Ctrl+S to save'}
        </p>
        {!note && (
          <button
            onClick={handleSave}
            disabled={!title.trim()}
            className="px-3 py-1.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 transition-colors"
          >
            Create Note
          </button>
        )}
      </div>
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add src/components/NoteEditor.tsx
git commit -m "feat: add NoteEditor component"
```

---

## Phase 8: Unified Search Integration

### Task 8.1: Update Backend Search to Include Notes and Files

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/search/mod.rs`

**Step 1: Add Note and File to ResultType**

In `src-tauri/src/search/mod.rs`, update ResultType:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResultType {
    Application,
    WebSearch,
    SystemCommand,
    Clipboard,
    Note,
    File,
}
```

**Step 2: Add OpenNote and OpenFile to SearchAction**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchAction {
    LaunchApp { path: String },
    OpenUrl { url: String },
    RunCommand { command: String },
    CopyClipboard { content: String },
    OpenNote { id: String },
    OpenFile { path: String },
}
```

**Step 3: Update search function in lib.rs**

Add notes and files to unified search (after clipboard check, before web search):

```rust
// Check for notes prefix
if query.starts_with("n:") || query.starts_with("n ") {
    let note_query = query.strip_prefix("n:").or_else(|| query.strip_prefix("n ")).unwrap_or("").trim();
    let notes = if note_query.is_empty() {
        state.notes.get_recent(8).unwrap_or_default()
    } else {
        state.notes.search(note_query).unwrap_or_default()
    };

    for note in notes {
        let preview = note.content.chars().take(60).collect::<String>().replace('\n', " ");
        items.push(SearchResult {
            id: note.id.clone(),
            name: note.title,
            description: preview,
            icon: Some("note".to_string()),
            result_type: ResultType::Note,
            score: 8000,
            action: SearchAction::OpenNote { id: note.id },
        });
    }

    // Return early for explicit note search
    results.truncate(settings.search.max_results);
    return results;
}

// Check for files prefix
if query.starts_with("f:") || query.starts_with("f ") {
    let file_query = query.strip_prefix("f:").or_else(|| query.strip_prefix("f ")).unwrap_or("").trim();
    if !file_query.is_empty() {
        let files = state.file_search.search(file_query).unwrap_or_default();
        for file in files {
            items.push(SearchResult {
                id: file.id,
                name: file.name,
                description: file.path.clone(),
                icon: Some("file".to_string()),
                result_type: ResultType::File,
                score: 7000,
                action: SearchAction::OpenFile { path: file.path },
            });
        }
    }

    results.truncate(settings.search.max_results);
    return results;
}
```

**Step 4: Add notes and files to unified search (without prefix)**

In the unified search section, add notes and files with lower priority:

```rust
// Add notes to unified search
if query.len() >= 2 {
    if let Ok(notes) = state.notes.search(&query) {
        for note in notes.into_iter().take(3) {
            let preview = note.content.chars().take(60).collect::<String>().replace('\n', " ");
            items.push(SearchResult {
                id: note.id.clone(),
                name: note.title,
                description: preview,
                icon: Some("note".to_string()),
                result_type: ResultType::Note,
                score: 0,
                action: SearchAction::OpenNote { id: note.id },
            });
        }
    }

    // Add files to unified search
    if settings.file_search.enabled {
        if let Ok(files) = state.file_search.search(&query) {
            for file in files.into_iter().take(3) {
                items.push(SearchResult {
                    id: file.id,
                    name: file.name,
                    description: file.path.clone(),
                    icon: Some("file".to_string()),
                    result_type: ResultType::File,
                    score: 0,
                    action: SearchAction::OpenFile { path: file.path },
                });
            }
        }
    }
}
```

**Step 5: Update execute_action to handle new action types**

```rust
#[tauri::command]
fn execute_action(action: SearchAction, state: State<AppState>) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => actions::launch_app(&path),
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
        SearchAction::CopyClipboard { content } => state.clipboard.copy_to_clipboard(&content),
        SearchAction::OpenNote { id } => {
            // Frontend handles opening note editor
            Ok(())
        }
        SearchAction::OpenFile { path } => open::that(&path).map_err(|e| e.to_string()),
    }
}
```

**Step 6: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles without errors

**Step 7: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/search/mod.rs
git commit -m "feat: integrate notes and files into unified search"
```

---

### Task 8.2: Update Frontend ResultItem for Notes and Files

**Files:**
- Modify: `src/components/ResultItem.tsx`

**Step 1: Read current ResultItem to understand structure**

**Step 2: Add rendering for note and file types**

Update the icon rendering to handle 'note' and 'file':

```tsx
// In the icon section
{result.icon === 'note' && (
  <span className="text-lg">📝</span>
)}
{result.icon === 'file' && (
  <span className="text-lg">📄</span>
)}
```

Update the type label:

```tsx
{result.result_type === 'note' && (
  <span className="text-xs text-purple-500 bg-purple-50 dark:bg-purple-900/30 px-1.5 py-0.5 rounded">
    Note
  </span>
)}
{result.result_type === 'file' && (
  <span className="text-xs text-green-500 bg-green-50 dark:bg-green-900/30 px-1.5 py-0.5 rounded">
    File
  </span>
)}
```

**Step 3: Commit**

```bash
git add src/components/ResultItem.tsx
git commit -m "feat: add note and file rendering to ResultItem"
```

---

### Task 8.3: Handle Note and File Actions in Frontend

**Files:**
- Modify: `src/stores/app.ts` or `src/components/ResultsList.tsx`

**Step 1: Update action execution**

When a note result is selected, open the note editor instead of calling backend:

```typescript
if (action.type === 'open_note') {
  const note = await invoke<Note>('get_note', { id: action.id });
  if (note) {
    setEditingNote(note);
  }
  return;
}

if (action.type === 'open_file') {
  await invoke('open_file', { path: action.path });
  return;
}
```

**Step 2: Add keyboard modifiers for file actions**

```typescript
// On Cmd/Ctrl+Enter, reveal file instead of open
if (action.type === 'open_file' && (e.metaKey || e.ctrlKey)) {
  await invoke('reveal_file', { path: action.path });
  return;
}
```

**Step 3: Commit**

```bash
git add src/stores/app.ts src/components/ResultsList.tsx
git commit -m "feat: handle note and file actions in frontend"
```

---

## Phase 9: Chained Shortcuts

### Task 9.1: Implement Chained Shortcuts in SearchBar

**Files:**
- Modify: `src/components/SearchBar.tsx`

**Step 1: Add keydown handler for empty search shortcuts**

```typescript
const handleKeyDown = (e: React.KeyboardEvent) => {
  const { query, setShowScratchpad, setNoteEditorVisible } = useAppStore.getState();

  // Only trigger on empty search
  if (query !== '') return;

  if (e.key === 's' || e.key === '`') {
    e.preventDefault();
    setShowScratchpad(true);
  }

  if (e.key === 'n') {
    e.preventDefault();
    setNoteEditorVisible(true);
  }

  // 'f' could focus on file search mode - optional
};
```

**Step 2: Wire up to input element**

```tsx
<input
  onKeyDown={handleKeyDown}
  // ... other props
/>
```

**Step 3: Commit**

```bash
git add src/components/SearchBar.tsx
git commit -m "feat: add chained shortcuts for scratchpad and notes"
```

---

## Phase 10: Settings UI Updates

### Task 10.1: Add File Search Settings UI

**Files:**
- Modify: `src/App.tsx` (SettingsPanel component)

**Step 1: Add File Search section**

```tsx
{/* File Search */}
<div>
  <div className="flex justify-between items-center mb-2">
    <label className="text-sm text-gray-500">File Search</label>
    <label className="flex items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={settings.file_search.enabled}
        onChange={(e) => saveSettings({
          ...settings,
          file_search: { ...settings.file_search, enabled: e.target.checked }
        })}
        className="rounded"
      />
      Enabled
    </label>
  </div>

  {settings.file_search.enabled && (
    <div className="space-y-2">
      <div>
        <label className="text-xs text-gray-400 block mb-1">Indexed Folders</label>
        {settings.file_search.indexed_paths.map((path, i) => (
          <div key={i} className="flex items-center gap-2 mb-1">
            <input
              type="text"
              value={path}
              onChange={(e) => {
                const newPaths = [...settings.file_search.indexed_paths];
                newPaths[i] = e.target.value;
                saveSettings({
                  ...settings,
                  file_search: { ...settings.file_search, indexed_paths: newPaths }
                });
              }}
              className="flex-1 px-2 py-1 text-sm bg-[var(--background)] border border-[var(--border)] rounded"
            />
            <button
              onClick={() => {
                const newPaths = settings.file_search.indexed_paths.filter((_, j) => j !== i);
                saveSettings({
                  ...settings,
                  file_search: { ...settings.file_search, indexed_paths: newPaths }
                });
              }}
              className="text-red-500 text-xs"
            >
              Remove
            </button>
          </div>
        ))}
        <button
          onClick={() => {
            saveSettings({
              ...settings,
              file_search: {
                ...settings.file_search,
                indexed_paths: [...settings.file_search.indexed_paths, '']
              }
            });
          }}
          className="text-xs text-blue-500"
        >
          + Add Folder
        </button>
      </div>

      <button
        onClick={async () => {
          const count = await invoke<number>('reindex_files');
          alert(`Indexed ${count} files`);
        }}
        className="px-3 py-1.5 rounded-lg text-sm bg-[var(--input-bg)] hover:bg-[var(--selected)] transition-colors"
      >
        Re-index Files
      </button>
    </div>
  )}
</div>
```

**Step 2: Commit**

```bash
git add src/App.tsx
git commit -m "feat: add file search settings UI"
```

---

## Phase 11: Final Integration & Testing

### Task 11.1: Integration Testing

**Step 1: Build and run the app**

```bash
npm run tauri dev
```

**Step 2: Test scratchpad**

- Type `scratch` or press `` ` `` - scratchpad should open
- Type some text - should auto-save
- Close and reopen - text should persist
- Click Clear - text should clear

**Step 3: Test notes**

- Type `n+ my first note` - note editor should open with title
- Add content with #tags
- Save and close
- Type `n: first` - should find the note
- Open and edit - should auto-save

**Step 4: Test file search**

- Go to Settings, add a folder to indexed paths
- Click Re-index
- Type `f: <filename>` - should find indexed files
- Press Enter - should open file
- Press Cmd/Ctrl+Enter - should reveal in Finder/Explorer

**Step 5: Test unified search**

- Type a word that matches an app, note, and file
- All three types should appear in results with correct labels

**Step 6: Test chained shortcuts**

- With empty search, press `s` - scratchpad opens
- With empty search, press `n` - note editor opens

**Step 7: Commit any fixes**

```bash
git add -A
git commit -m "fix: integration testing fixes"
```

---

### Task 11.2: Final Build Verification

**Step 1: Run full build**

```bash
npm run build
cd src-tauri && cargo build --release
```

**Step 2: Run tests**

```bash
cd src-tauri && cargo test
npm test
```

**Step 3: Final commit**

```bash
git add -A
git commit -m "feat: complete notes and file search implementation"
```

---

## Summary

This plan implements:

1. **Scratchpad** - Ephemeral text buffer with `` ` `` or `scratch` trigger
2. **Notes** - Full CRUD with tags, `n:` search, `n+` create
3. **File Search** - Configurable indexed folders, `f:` search
4. **Unified Search** - Notes and files appear alongside apps
5. **Chained Shortcuts** - Single-key triggers on empty search
6. **Settings UI** - File search configuration

Total: ~30 tasks, each 2-5 minutes
