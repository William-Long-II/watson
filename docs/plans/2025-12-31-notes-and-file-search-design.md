# Watson v2: Notes & File Search

## Overview

Extend Watson's unified search to include persistent notes, an ephemeral scratchpad, and file search. All sources (apps, web searches, notes, files, commands) are searchable from a single input with intelligent ranking.

## Goals

- **Unified search** - One input searches everything
- **Quick capture** - Jot notes without leaving the launcher
- **File discovery** - Find files across user-configured folders
- **Keyboard-first** - Chained shortcuts for power users
- **Minimal friction** - Auto-save, sensible defaults

## UX Flow

### Search Behavior

| Input | Behavior |
|-------|----------|
| `chrome` | Apps, files, notes matching "chrome" |
| `n: meeting notes` | Search notes only |
| `n+ standup agenda` | Quick-create a new note |
| `n` (empty search) | Shows recent notes chronologically |
| `f: invoice` | Search files only |
| `f: .pdf` | Filter files by extension |
| `scratch` or `` ` `` | Open ephemeral scratchpad |
| `s` (empty search) | Open scratchpad (chained shortcut) |
| `cb` | Clipboard history (existing) |
| `>lock` | System commands (existing) |

### Result Ranking (Unified Mode)

1. Exact prefix matches (apps prioritized for short queries)
2. Frecency score per source type
3. Recency boost (recently opened files/notes rank higher)
4. Source weighting: apps > notes > files

### Keyboard Flow

- Type query → results appear mixed from all sources
- Each result shows its source (icon + subtle label)
- Enter → execute action (launch app, open note, open file)
- Tab/Shift+Tab → cycle results (existing)

### Chained Shortcuts

When search is empty, single keys jump to modes:
- `n` → Notes mode
- `s` → Scratchpad
- `f` → Files mode

Future: Configurable global hotkeys (Alt+Shift+N, etc.) as power-user settings.

---

## Notes Feature

### Two Modes

**Persistent Notes** - Searchable, titled notes stored permanently
**Scratchpad** - Single ephemeral buffer that clears when dismissed

### Creating Notes

| Input | Action |
|-------|--------|
| `n+ meeting notes` | Creates note titled "meeting notes", opens editor |
| `n: meeting` | Searches existing notes for "meeting" |
| `n` (empty search) | Shows recent notes chronologically |

### Note Structure

- **Title** (required, from creation command or first line)
- **Content** (markdown-supported plain text)
- **Tags** (optional, inline #hashtags parsed from content)
- **Created timestamp**
- **Modified timestamp**

### Editor Behavior

- Inline editing in Watson window (no external app)
- Auto-saves on blur/dismiss (no save button needed)
- Minimal UI: just the text, subtle title bar
- Escape or click outside → saves and returns to search
- Markdown rendering optional (toggle in settings)

### Finding Notes

- Full-text search on title + content
- Filter by tag: `n: #work`
- Chronological browse: `n` with empty query shows recent first
- Notes appear in unified search alongside apps/files

---

## Scratchpad

### Purpose

Quick temporary buffer for holding text - copying between apps, drafting a message, parking a value temporarily.

### Access

| Input | Action |
|-------|--------|
| `scratch` | Opens scratchpad |
| `` ` `` (backtick) | Opens scratchpad |
| `s` (empty search) | Opens scratchpad (chained shortcut) |

### Behavior

- **Single buffer** - Only one scratchpad at a time
- **Persists across sessions** - Content survives app restart
- **Manual clear** - "Clear" button or Cmd/Ctrl+Shift+Delete
- **No title, no tags** - Just raw text, zero friction
- **Auto-focus** - Opens with cursor ready to type

### UI

```
┌────────────────────────────────────────┐
│  Scratchpad                 [Clear] X  │
├────────────────────────────────────────┤
│                                        │
│  [Your temporary text here...]         │
│                                        │
└────────────────────────────────────────┘
```

---

## File Search

### Scope Configuration

Users define which folders to index in settings:

```toml
[file_search]
enabled = true
indexed_paths = [
  "~/Downloads",
  "~/Documents",
  "D:/Projects",
]
excluded_patterns = [
  "node_modules",
  ".git",
  "target",
  "*.tmp",
]
max_depth = 10
```

### Indexing Strategy

- **Background indexing** - Runs on startup, doesn't block UI
- **File watcher** - Detects new/changed/deleted files in real-time
- **Incremental updates** - Only re-index changed files
- **Metadata stored** - Name, path, extension, size, modified date

### Search Behavior

| Input | Action |
|-------|--------|
| `f: invoice` | Searches files only |
| `f: .pdf` | Filter by extension |
| `f: ~/Downloads` | Scope to specific folder |
| `invoice.pdf` | Unified search (files + apps + notes) |

### Result Display

```
┌────────────────────────────────────────┐
│  invoice-2024.pdf                      │
│  ~/Downloads · 2.3 MB · Today          │
└────────────────────────────────────────┘
```

### Actions on File Results

- **Enter** - Open file with default app
- **Cmd/Ctrl+Enter** - Reveal in Finder/Explorer
- **Cmd/Ctrl+C** - Copy file path

---

## Settings

### File Search Settings

```
┌─ File Search ─────────────────────────────┐
│                                           │
│  [x] Enable file search                   │
│                                           │
│  Indexed Folders:                         │
│  ┌─────────────────────────────────────┐  │
│  │ ~/Downloads                    [X]  │  │
│  │ ~/Documents                    [X]  │  │
│  │ D:/Projects                    [X]  │  │
│  └─────────────────────────────────────┘  │
│  [+ Add Folder]                           │
│                                           │
│  Excluded Patterns:                       │
│  node_modules, .git, target, *.tmp        │
│  [Edit]                                   │
│                                           │
│  Max Folder Depth: [10]                   │
│                                           │
│  [Re-index Now]     Last indexed: 2m ago  │
└───────────────────────────────────────────┘
```

### Notes Settings

```
┌─ Notes ───────────────────────────────────┐
│                                           │
│  Storage location:                        │
│  ~/Documents/Watson Notes        [Change] │
│                                           │
│  [ ] Render markdown preview              │
│  [x] Parse #hashtags as searchable tags   │
│                                           │
│  [Export All Notes]  [Open Notes Folder]  │
└───────────────────────────────────────────┘
```

### Config File Additions

```toml
[notes]
storage_path = "~/Documents/Watson Notes"
render_markdown = false
parse_hashtags = true

[scratchpad]
persist_across_restart = true

[file_search]
enabled = true
indexed_paths = ["~/Downloads", "~/Documents"]
excluded_patterns = ["node_modules", ".git", "target"]
max_depth = 10
```

---

## Data Storage

### New Database Tables

```sql
-- Notes storage
CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  modified_at INTEGER NOT NULL
);

CREATE TABLE note_tags (
  note_id TEXT NOT NULL,
  tag TEXT NOT NULL,
  PRIMARY KEY (note_id, tag),
  FOREIGN KEY (note_id) REFERENCES notes(id) ON DELETE CASCADE
);

-- File index cache
CREATE TABLE files (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  extension TEXT,
  size_bytes INTEGER,
  modified_at INTEGER NOT NULL,
  indexed_at INTEGER NOT NULL
);

-- Scratchpad (single row)
CREATE TABLE scratchpad (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  content TEXT NOT NULL DEFAULT '',
  modified_at INTEGER NOT NULL
);

-- Indexes
CREATE INDEX idx_notes_title ON notes(title);
CREATE INDEX idx_notes_modified ON notes(modified_at);
CREATE INDEX idx_note_tags_tag ON note_tags(tag);
CREATE INDEX idx_files_name ON files(name);
CREATE INDEX idx_files_extension ON files(extension);
CREATE INDEX idx_files_modified ON files(modified_at);
```

### Storage Locations

| Data | Location |
|------|----------|
| Database | `~/Library/Application Support/Watson/watson.db` (macOS) |
| Note files | `~/Documents/Watson Notes/*.md` (configurable) |
| Config | `config.toml` (existing) |

### Notes Dual Storage

Notes stored as both database rows (fast search) and markdown files (portability). Sync on startup and file watch for external edits.

---

## Architecture

### New Rust Modules

```
src-tauri/src/
├── notes/
│   ├── mod.rs          # Note CRUD, search
│   ├── storage.rs      # File + DB sync
│   └── tags.rs         # Hashtag parsing
├── files/
│   ├── mod.rs          # File search queries
│   ├── indexer.rs      # Background indexing
│   └── watcher.rs      # Filesystem watcher
├── scratchpad.rs       # Simple get/set
└── search/
    └── mod.rs          # Updated: unified ranking
```

### New Tauri Commands

```rust
// Notes
#[tauri::command] fn create_note(title, content) -> Note
#[tauri::command] fn update_note(id, title, content) -> Note
#[tauri::command] fn delete_note(id)
#[tauri::command] fn search_notes(query) -> Vec<Note>
#[tauri::command] fn get_recent_notes(limit) -> Vec<Note>

// Files
#[tauri::command] fn search_files(query) -> Vec<FileResult>
#[tauri::command] fn open_file(path)
#[tauri::command] fn reveal_file(path)
#[tauri::command] fn reindex_files()

// Scratchpad
#[tauri::command] fn get_scratchpad() -> String
#[tauri::command] fn set_scratchpad(content)
#[tauri::command] fn clear_scratchpad()

// Unified
#[tauri::command] fn unified_search(query) -> UnifiedResults
```

### Frontend Changes

- New result types in `types.ts`: `NoteResult`, `FileResult`
- Updated `ResultItem.tsx`: render note/file variants
- New components: `NoteEditor.tsx`, `Scratchpad.tsx`
- Updated store: note/file state management

---

## Future Considerations

- Global hotkeys (Alt+Shift+N for notes, etc.) as power-user settings
- Note templates
- File content search (not just filenames)
- Note linking/backlinks
- Cloud sync for notes
