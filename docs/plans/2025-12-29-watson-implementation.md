# Watson Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a cross-platform productivity launcher (Alfred alternative) for macOS and Windows with app launching, web search, and system commands.

**Architecture:** Tauri app with Rust backend handling indexing, search, and actions; React/TypeScript frontend for UI. SQLite for data persistence, TOML for configuration. Clean internal interfaces between components for future extensibility.

**Tech Stack:** Tauri 2.x, Rust, React 18, TypeScript, Vite, Tailwind CSS, Zustand, SQLite (rusqlite), TOML (toml crate)

**Testing Strategy:** Each phase includes verification steps. After each task:
1. Code must compile (`cargo check` / `npm run build`)
2. Existing tests must pass (`cargo test` / `npm test`)
3. New functionality has corresponding tests
4. Full app can start (`npm run tauri dev`)

---

## Phase 1: Project Scaffolding

### Task 1: Initialize Tauri Project

**Files:**
- Create: `package.json`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`
- Create: `src/main.tsx`
- Create: `src/App.tsx`

**Step 1: Create Tauri project with React template**

Run:
```bash
npm create tauri-app@latest . -- --template react-ts --manager npm
```

Expected: Project scaffolded with Tauri + React + TypeScript

**Step 2: Verify project structure exists**

Run:
```bash
ls -la src-tauri/src/ && ls -la src/
```

Expected: `main.rs` in src-tauri/src, `main.tsx` and `App.tsx` in src

**Step 3: Install dependencies**

Run:
```bash
npm install
```

Expected: node_modules created, no errors

**Step 4: Verify dev server starts**

Run:
```bash
npm run tauri dev &
sleep 15
pkill -f "tauri dev" || true
```

Expected: App window opens (or build starts successfully)

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: initialize Tauri project with React template"
```

---

### Task 2: Add Frontend Dependencies

**Files:**
- Modify: `package.json`
- Create: `tailwind.config.js`
- Create: `postcss.config.js`
- Modify: `src/index.css`

**Step 1: Install Tailwind CSS and Zustand**

Run:
```bash
npm install -D tailwindcss postcss autoprefixer
npm install zustand
npx tailwindcss init -p
```

Expected: Dependencies added, tailwind.config.js and postcss.config.js created

**Step 2: Configure Tailwind**

Replace `tailwind.config.js`:
```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {},
  },
  plugins: [],
}
```

**Step 3: Add Tailwind directives to CSS**

Replace `src/index.css`:
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  --background: #ffffff;
  --foreground: #1a1a1a;
  --border: #e5e5e5;
  --selected: #f0f0f0;
  --input-bg: #f5f5f5;
}

.dark {
  --background: #1a1a2e;
  --foreground: #eaeaea;
  --border: #333355;
  --selected: #4a4a6a;
  --input-bg: #252540;
}

body {
  background-color: var(--background);
  color: var(--foreground);
  font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}
```

**Step 4: Verify Tailwind works**

Replace `src/App.tsx`:
```tsx
function App() {
  return (
    <div className="p-4 bg-[var(--background)] text-[var(--foreground)]">
      <h1 className="text-2xl font-bold">Watson</h1>
      <p className="text-gray-500">Launcher initializing...</p>
    </div>
  );
}

export default App;
```

**Step 5: Run dev to verify styles**

Run:
```bash
npm run dev &
sleep 5
pkill -f "vite" || true
```

Expected: No build errors

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add Tailwind CSS and Zustand"
```

---

### Task 3: Configure Tauri for Launcher Behavior

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/Cargo.toml`

**Step 1: Update tauri.conf.json for launcher window**

Read current file first, then update the `windows` section in `src-tauri/tauri.conf.json`:
```json
{
  "app": {
    "windows": [
      {
        "title": "Watson",
        "width": 600,
        "height": 400,
        "resizable": false,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "center": true,
        "visible": false
      }
    ],
    "security": {
      "csp": null
    }
  }
}
```

**Step 2: Add required Rust dependencies to Cargo.toml**

Add to `[dependencies]` section in `src-tauri/Cargo.toml`:
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
rusqlite = { version = "0.31", features = ["bundled"] }
toml = "0.8"
directories = "5.0"
fuzzy-matcher = "0.3"
```

**Step 3: Verify Cargo dependencies resolve**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: Dependencies downloaded, no errors

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: configure Tauri window and add Rust dependencies"
```

---

## Phase 2: Core Backend Infrastructure

### Task 4: Create Config Module

**Files:**
- Create: `src-tauri/src/config/mod.rs`
- Create: `src-tauri/src/config/settings.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Create config directory**

Run:
```bash
mkdir -p src-tauri/src/config
```

**Step 2: Create settings struct**

Create `src-tauri/src/config/settings.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub general: GeneralSettings,
    pub activation: ActivationSettings,
    pub search: SearchSettings,
    pub theme: ThemeSettings,
    #[serde(default)]
    pub web_searches: Vec<WebSearch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    #[serde(default = "default_true")]
    pub launch_at_login: bool,
    #[serde(default)]
    pub show_in_dock: bool,
    #[serde(default)]
    pub show_in_taskbar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationSettings {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSettings {
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default = "default_true")]
    pub show_recently_used: bool,
    #[serde(default = "default_threshold")]
    pub fuzzy_match_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    pub custom: Option<CustomTheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTheme {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub border: Option<String>,
    pub selected_background: Option<String>,
    pub input_background: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<u32>,
    pub border_radius: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearch {
    pub name: String,
    pub keyword: String,
    pub url: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub requires_setup: bool,
    #[serde(default)]
    pub instance: Option<String>,
}

fn default_true() -> bool { true }
fn default_hotkey() -> String { "Alt+Space".to_string() }
fn default_max_results() -> usize { 8 }
fn default_threshold() -> f64 { 0.6 }
fn default_mode() -> String { "system".to_string() }
fn default_accent() -> String { "system".to_string() }

impl Default for Settings {
    fn default() -> Self {
        Settings {
            general: GeneralSettings {
                launch_at_login: true,
                show_in_dock: false,
                show_in_taskbar: false,
            },
            activation: ActivationSettings {
                hotkey: default_hotkey(),
                show_tray_icon: true,
            },
            search: SearchSettings {
                max_results: 8,
                show_recently_used: true,
                fuzzy_match_threshold: 0.6,
            },
            theme: ThemeSettings {
                mode: "system".to_string(),
                accent_color: "system".to_string(),
                custom: None,
            },
            web_searches: default_web_searches(),
        }
    }
}

fn default_web_searches() -> Vec<WebSearch> {
    vec![
        WebSearch {
            name: "Google".to_string(),
            keyword: "g".to_string(),
            url: "https://www.google.com/search?q={query}".to_string(),
            icon: Some("google".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "DuckDuckGo".to_string(),
            keyword: "ddg".to_string(),
            url: "https://duckduckgo.com/?q={query}".to_string(),
            icon: Some("duckduckgo".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "YouTube".to_string(),
            keyword: "yt".to_string(),
            url: "https://www.youtube.com/results?search_query={query}".to_string(),
            icon: Some("youtube".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "GitHub".to_string(),
            keyword: "gh".to_string(),
            url: "https://github.com/search?q={query}".to_string(),
            icon: Some("github".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "Wikipedia".to_string(),
            keyword: "wiki".to_string(),
            url: "https://en.wikipedia.org/wiki/Special:Search?search={query}".to_string(),
            icon: Some("wikipedia".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "Stack Overflow".to_string(),
            keyword: "so".to_string(),
            url: "https://stackoverflow.com/search?q={query}".to_string(),
            icon: Some("stackoverflow".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "Jira".to_string(),
            keyword: "jira".to_string(),
            url: "https://{instance}.atlassian.net/browse/{query}".to_string(),
            icon: Some("jira".to_string()),
            requires_setup: true,
            instance: None,
        },
    ]
}
```

**Step 3: Create config module**

Create `src-tauri/src/config/mod.rs`:
```rust
pub mod settings;

use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;
use crate::config::settings::Settings;

pub fn get_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn get_config_path() -> Option<PathBuf> {
    get_config_dir().map(|dir| dir.join("config.toml"))
}

pub fn load_settings() -> Settings {
    let path = match get_config_path() {
        Some(p) => p,
        None => return Settings::default(),
    };

    if !path.exists() {
        let settings = Settings::default();
        let _ = save_settings(&settings);
        return settings;
    }

    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let dir = get_config_dir().ok_or("Could not determine config directory")?;
    let path = dir.join("config.toml");

    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let content = toml::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;

    Ok(())
}
```

**Step 4: Update main.rs to include config module**

Replace `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;

use config::settings::Settings;

#[tauri::command]
fn get_settings() -> Settings {
    config::load_settings()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    config::save_settings(&settings)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 5: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add config module with settings management"
```

---

### Task 5: Create Database Module

**Files:**
- Create: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/db/schema.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Create db directory**

Run:
```bash
mkdir -p src-tauri/src/db
```

**Step 2: Create schema module**

Create `src-tauri/src/db/schema.rs`:
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

CREATE INDEX IF NOT EXISTS idx_apps_name ON apps(name);
CREATE INDEX IF NOT EXISTS idx_history_query ON search_history(query);
CREATE INDEX IF NOT EXISTS idx_history_timestamp ON search_history(timestamp);
"#;
```

**Step 3: Create db module**

Create `src-tauri/src/db/mod.rs`:
```rust
pub mod schema;

use rusqlite::{Connection, Result};
use std::path::PathBuf;
use std::sync::Mutex;
use directories::ProjectDirs;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new() -> Result<Self> {
        let path = get_db_path().expect("Could not determine database path");

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(&path)?;
        conn.execute_batch(schema::SCHEMA)?;

        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(sql, params)
    }

    pub fn query_map<T, F>(&self, sql: &str, params: &[&dyn rusqlite::ToSql], f: F) -> Result<Vec<T>>
    where
        F: FnMut(&rusqlite::Row<'_>) -> Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, f)?;
        rows.collect()
    }
}

fn get_db_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.data_dir().join("watson.db"))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub icon_cache_path: Option<String>,
    pub launch_count: i32,
    pub last_launched: Option<i64>,
    pub platform: String,
}
```

**Step 4: Update main.rs to include db module**

Update `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod db;

use config::settings::Settings;
use db::Database;
use std::sync::Arc;
use tauri::State;

struct AppState {
    db: Arc<Database>,
}

#[tauri::command]
fn get_settings() -> Settings {
    config::load_settings()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    config::save_settings(&settings)
}

fn main() {
    let db = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
        .manage(AppState { db: Arc::new(db) })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 5: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add database module with SQLite schema"
```

---

### Task 6: Create Search Engine Module

**Files:**
- Create: `src-tauri/src/search/mod.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Create search directory**

Run:
```bash
mkdir -p src-tauri/src/search
```

**Step 2: Create search module**

Create `src-tauri/src/search/mod.rs`:
```rust
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub result_type: ResultType,
    pub score: i64,
    pub action: SearchAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultType {
    Application,
    WebSearch,
    SystemCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchAction {
    LaunchApp { path: String },
    OpenUrl { url: String },
    RunCommand { command: String },
}

pub struct SearchEngine {
    matcher: SkimMatcherV2,
}

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn score(&self, query: &str, target: &str) -> Option<i64> {
        self.matcher.fuzzy_match(target, query)
    }

    pub fn search(&self, query: &str, items: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut results: Vec<(SearchResult, i64)> = items
            .into_iter()
            .filter_map(|mut item| {
                self.score(query, &item.name).map(|score| {
                    item.score = score;
                    (item, score)
                })
            })
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.into_iter().map(|(item, _)| item).collect()
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 3: Update main.rs to include search module**

Update `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod db;
mod search;

use config::settings::Settings;
use db::Database;
use search::{SearchEngine, SearchResult};
use std::sync::Arc;
use tauri::State;

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
}

#[tauri::command]
fn get_settings() -> Settings {
    config::load_settings()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    config::save_settings(&settings)
}

#[tauri::command]
fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    // TODO: Gather items from indexers and web searches
    let items = vec![];
    state.search_engine.search(&query, items)
}

fn main() {
    let db = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
        })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 4: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add search engine with fuzzy matching"
```

---

### Task 6.5: Test Phase 2 - Core Backend

**Files:**
- Create: `src-tauri/src/config/tests.rs`
- Create: `src-tauri/src/search/tests.rs`

**Step 1: Add config module tests**

Create `src-tauri/src/config/tests.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::Settings;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.activation.hotkey, "Alt+Space");
        assert_eq!(settings.search.max_results, 8);
        assert!(settings.general.launch_at_login);
    }

    #[test]
    fn test_default_web_searches_count() {
        let settings = Settings::default();
        assert!(settings.web_searches.len() >= 6);
    }

    #[test]
    fn test_web_search_keywords_unique() {
        let settings = Settings::default();
        let keywords: Vec<_> = settings.web_searches.iter().map(|w| &w.keyword).collect();
        let unique: std::collections::HashSet<_> = keywords.iter().collect();
        assert_eq!(keywords.len(), unique.len(), "Web search keywords must be unique");
    }
}
```

**Step 2: Add mod tests to config/mod.rs**

Add at the end of `src-tauri/src/config/mod.rs`:
```rust
#[cfg(test)]
mod tests;
```

**Step 3: Add search module tests**

Create `src-tauri/src/search/tests.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_engine_creation() {
        let engine = SearchEngine::new();
        assert!(engine.score("chr", "Chrome").is_some());
    }

    #[test]
    fn test_fuzzy_match_scores() {
        let engine = SearchEngine::new();

        // Exact prefix should score higher
        let chrome_score = engine.score("chr", "Chrome").unwrap();
        let chromium_score = engine.score("chr", "Chromium").unwrap();

        // Both should match
        assert!(chrome_score > 0);
        assert!(chromium_score > 0);
    }

    #[test]
    fn test_no_match_returns_none() {
        let engine = SearchEngine::new();
        assert!(engine.score("xyz", "Chrome").is_none());
    }

    #[test]
    fn test_search_filters_and_sorts() {
        let engine = SearchEngine::new();

        let items = vec![
            SearchResult {
                id: "1".to_string(),
                name: "Chrome".to_string(),
                description: "Browser".to_string(),
                icon: None,
                result_type: ResultType::Application,
                score: 0,
                action: SearchAction::LaunchApp { path: "/app".to_string() },
            },
            SearchResult {
                id: "2".to_string(),
                name: "Firefox".to_string(),
                description: "Browser".to_string(),
                icon: None,
                result_type: ResultType::Application,
                score: 0,
                action: SearchAction::LaunchApp { path: "/app".to_string() },
            },
        ];

        let results = engine.search("chr", items);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Chrome");
    }
}
```

**Step 4: Add mod tests to search/mod.rs**

Add at the end of `src-tauri/src/search/mod.rs`:
```rust
#[cfg(test)]
mod tests;
```

**Step 5: Run all tests**

Run:
```bash
cd src-tauri && cargo test && cd ..
```

Expected: All tests pass

**Step 6: Verify full build**

Run:
```bash
npm run build
```

Expected: Build succeeds

**Step 7: Commit**

```bash
git add -A
git commit -m "test: add unit tests for config and search modules"
```

---

## Phase 3: Platform Indexers

### Task 7: Create App Indexer Trait and macOS Implementation

**Files:**
- Create: `src-tauri/src/indexers/mod.rs`
- Create: `src-tauri/src/indexers/macos.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Create indexers directory**

Run:
```bash
mkdir -p src-tauri/src/indexers
```

**Step 2: Create indexer trait and module**

Create `src-tauri/src/indexers/mod.rs`:
```rust
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use crate::db::AppEntry;

pub trait AppIndexer: Send + Sync {
    fn index_apps(&self) -> Vec<AppEntry>;
    fn get_app_icon(&self, app: &AppEntry) -> Option<Vec<u8>>;
}

#[cfg(target_os = "macos")]
pub fn get_indexer() -> impl AppIndexer {
    macos::MacOSIndexer::new()
}

#[cfg(target_os = "windows")]
pub fn get_indexer() -> impl AppIndexer {
    windows::WindowsIndexer::new()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn get_indexer() -> impl AppIndexer {
    StubIndexer
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct StubIndexer;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl AppIndexer for StubIndexer {
    fn index_apps(&self) -> Vec<AppEntry> { vec![] }
    fn get_app_icon(&self, _app: &AppEntry) -> Option<Vec<u8>> { None }
}
```

**Step 3: Create macOS indexer**

Create `src-tauri/src/indexers/macos.rs`:
```rust
use crate::db::AppEntry;
use crate::indexers::AppIndexer;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct MacOSIndexer {
    search_paths: Vec<PathBuf>,
}

impl MacOSIndexer {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        MacOSIndexer {
            search_paths: vec![
                PathBuf::from("/Applications"),
                PathBuf::from("/System/Applications"),
                PathBuf::from(format!("{}/Applications", home)),
            ],
        }
    }

    fn scan_directory(&self, path: &PathBuf) -> Vec<AppEntry> {
        let mut apps = Vec::new();

        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(_) => return apps,
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "app").unwrap_or(false) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let id = format!("app:{}", path.display());
                    apps.push(AppEntry {
                        id,
                        name: name.to_string(),
                        path: path.display().to_string(),
                        icon_cache_path: None,
                        launch_count: 0,
                        last_launched: None,
                        platform: "macos".to_string(),
                    });
                }
            }
        }

        apps
    }
}

impl AppIndexer for MacOSIndexer {
    fn index_apps(&self) -> Vec<AppEntry> {
        let mut all_apps = Vec::new();
        for path in &self.search_paths {
            all_apps.extend(self.scan_directory(path));
        }
        all_apps
    }

    fn get_app_icon(&self, _app: &AppEntry) -> Option<Vec<u8>> {
        // TODO: Extract icon from .app bundle
        None
    }
}
```

**Step 4: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors (may show warnings about unused code on non-macOS)

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add app indexer trait and macOS implementation"
```

---

### Task 8: Create Windows Indexer

**Files:**
- Create: `src-tauri/src/indexers/windows.rs`

**Step 1: Create Windows indexer**

Create `src-tauri/src/indexers/windows.rs`:
```rust
use crate::db::AppEntry;
use crate::indexers::AppIndexer;
use std::fs;
use std::path::PathBuf;

pub struct WindowsIndexer {
    search_paths: Vec<PathBuf>,
}

impl WindowsIndexer {
    pub fn new() -> Self {
        let program_data = std::env::var("ProgramData").unwrap_or_default();
        let app_data = std::env::var("APPDATA").unwrap_or_default();

        WindowsIndexer {
            search_paths: vec![
                PathBuf::from(format!(
                    "{}\\Microsoft\\Windows\\Start Menu\\Programs",
                    program_data
                )),
                PathBuf::from(format!(
                    "{}\\Microsoft\\Windows\\Start Menu\\Programs",
                    app_data
                )),
            ],
        }
    }

    fn scan_directory(&self, path: &PathBuf) -> Vec<AppEntry> {
        let mut apps = Vec::new();
        self.scan_directory_recursive(path, &mut apps);
        apps
    }

    fn scan_directory_recursive(&self, path: &PathBuf, apps: &mut Vec<AppEntry>) {
        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                self.scan_directory_recursive(&path, apps);
            } else if path.extension().map(|e| e == "lnk").unwrap_or(false) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    // Skip uninstall shortcuts
                    if name.to_lowercase().contains("uninstall") {
                        continue;
                    }

                    let id = format!("app:{}", path.display());
                    apps.push(AppEntry {
                        id,
                        name: name.to_string(),
                        path: path.display().to_string(),
                        icon_cache_path: None,
                        launch_count: 0,
                        last_launched: None,
                        platform: "windows".to_string(),
                    });
                }
            }
        }
    }
}

impl AppIndexer for WindowsIndexer {
    fn index_apps(&self) -> Vec<AppEntry> {
        let mut all_apps = Vec::new();
        for path in &self.search_paths {
            all_apps.extend(self.scan_directory(path));
        }
        all_apps
    }

    fn get_app_icon(&self, _app: &AppEntry) -> Option<Vec<u8>> {
        // TODO: Extract icon from .exe or .lnk
        None
    }
}
```

**Step 2: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add Windows app indexer"
```

---

### Task 9: Integrate Indexer with Search

**Files:**
- Modify: `src-tauri/src/main.rs`

**Step 1: Update main.rs to use indexer**

Update `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod db;
mod indexers;
mod search;

use config::settings::Settings;
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
use search::{ResultType, SearchAction, SearchEngine, SearchResult};
use std::sync::{Arc, RwLock};
use tauri::State;

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings_cmd(settings: Settings, state: State<AppState>) -> Result<(), String> {
    config::save_settings(&settings)?;
    *state.settings.write().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn reindex_apps(state: State<AppState>) -> usize {
    let indexer = get_indexer();
    let apps = indexer.index_apps();
    let count = apps.len();
    *state.indexed_apps.write().unwrap() = apps;
    count
}

#[tauri::command]
fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    if query.is_empty() {
        return vec![];
    }

    let settings = state.settings.read().unwrap();
    let apps = state.indexed_apps.read().unwrap();

    // Convert apps to search results
    let mut items: Vec<SearchResult> = apps
        .iter()
        .map(|app| SearchResult {
            id: app.id.clone(),
            name: app.name.clone(),
            description: "Application".to_string(),
            icon: app.icon_cache_path.clone(),
            result_type: ResultType::Application,
            score: 0,
            action: SearchAction::LaunchApp {
                path: app.path.clone(),
            },
        })
        .collect();

    // Add web search results
    for ws in &settings.web_searches {
        if query.starts_with(&format!("{} ", ws.keyword)) {
            let search_query = query.strip_prefix(&format!("{} ", ws.keyword)).unwrap();
            let url = ws.url.replace("{query}", &urlencoding::encode(search_query));
            items.insert(
                0,
                SearchResult {
                    id: format!("web:{}", ws.keyword),
                    name: format!("{}: {}", ws.name, search_query),
                    description: "Web Search".to_string(),
                    icon: ws.icon.clone(),
                    result_type: ResultType::WebSearch,
                    score: 1000,
                    action: SearchAction::OpenUrl { url },
                },
            );
        }
    }

    // Search and limit results
    let mut results = state.search_engine.search(&query, items);
    results.truncate(settings.search.max_results);
    results
}

fn main() {
    let db = Database::new().expect("Failed to initialize database");
    let settings = config::load_settings();
    let indexer = get_indexer();
    let indexed_apps = indexer.index_apps();

    tauri::Builder::default()
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
            indexed_apps: RwLock::new(indexed_apps),
            settings: RwLock::new(settings),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings_cmd,
            reindex_apps,
            search
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 2: Add urlencoding dependency**

Add to `src-tauri/Cargo.toml` dependencies:
```toml
urlencoding = "2.1"
```

**Step 3: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: integrate app indexer with search"
```

---

### Task 9.5: Test Phase 3 - Platform Indexers

**Files:**
- Create: `src-tauri/src/indexers/tests.rs`

**Step 1: Add indexer tests**

Create `src-tauri/src/indexers/tests.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::AppEntry;

    #[test]
    fn test_indexer_trait_exists() {
        // Verify the indexer can be created
        let _indexer = get_indexer();
    }

    #[test]
    fn test_indexer_returns_apps() {
        let indexer = get_indexer();
        let apps = indexer.index_apps();

        // Should return some apps (may be empty in test environments)
        // Just verify it doesn't panic
        let _ = apps.len();
    }

    #[test]
    fn test_app_entry_fields() {
        let entry = AppEntry {
            id: "test:app".to_string(),
            name: "Test App".to_string(),
            path: "/path/to/app".to_string(),
            icon_cache_path: None,
            launch_count: 0,
            last_launched: None,
            platform: "test".to_string(),
        };

        assert_eq!(entry.name, "Test App");
        assert!(entry.id.starts_with("test:"));
    }
}
```

**Step 2: Add mod tests to indexers/mod.rs**

Add at the end of `src-tauri/src/indexers/mod.rs`:
```rust
#[cfg(test)]
mod tests;
```

**Step 3: Run all tests**

Run:
```bash
cd src-tauri && cargo test && cd ..
```

Expected: All tests pass

**Step 4: Verify app starts and indexes**

Run:
```bash
npm run tauri dev &
sleep 20
pkill -f "tauri" || true
```

Expected: App starts without errors

**Step 5: Commit**

```bash
git add -A
git commit -m "test: add unit tests for indexer module"
```

---

## Phase 4: System Commands

### Task 10: Create Actions Module with System Commands

**Files:**
- Create: `src-tauri/src/actions/mod.rs`
- Create: `src-tauri/src/actions/system.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Create actions directory**

Run:
```bash
mkdir -p src-tauri/src/actions
```

**Step 2: Create system commands module**

Create `src-tauri/src/actions/system.rs`:
```rust
use std::process::Command;

#[derive(Debug, Clone)]
pub struct SystemCommand {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub requires_confirmation: bool,
}

pub fn get_system_commands() -> Vec<SystemCommand> {
    vec![
        SystemCommand {
            id: "cmd:lock".to_string(),
            name: "Lock".to_string(),
            aliases: vec!["lock".to_string(), "lockscreen".to_string()],
            description: "Lock the screen".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:sleep".to_string(),
            name: "Sleep".to_string(),
            aliases: vec!["sleep".to_string()],
            description: "Put computer to sleep".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:restart".to_string(),
            name: "Restart".to_string(),
            aliases: vec!["restart".to_string(), "reboot".to_string()],
            description: "Restart the computer".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:shutdown".to_string(),
            name: "Shutdown".to_string(),
            aliases: vec!["shutdown".to_string(), "poweroff".to_string()],
            description: "Shut down the computer".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:logout".to_string(),
            name: "Log Out".to_string(),
            aliases: vec!["logout".to_string(), "signout".to_string()],
            description: "Log out current user".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:emptytrash".to_string(),
            name: "Empty Trash".to_string(),
            aliases: vec!["emptytrash".to_string(), "trash".to_string()],
            description: "Empty the trash/recycle bin".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:mute".to_string(),
            name: "Mute".to_string(),
            aliases: vec!["mute".to_string()],
            description: "Mute system audio".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:unmute".to_string(),
            name: "Unmute".to_string(),
            aliases: vec!["unmute".to_string()],
            description: "Unmute system audio".to_string(),
            requires_confirmation: false,
        },
    ]
}

#[cfg(target_os = "macos")]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    match command_id {
        "cmd:lock" => {
            Command::new("pmset")
                .args(["displaysleepnow"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:sleep" => {
            Command::new("pmset")
                .args(["sleepnow"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:restart" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to restart"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:shutdown" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to shut down"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:logout" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to log out"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:emptytrash" => {
            Command::new("osascript")
                .args(["-e", "tell app \"Finder\" to empty the trash"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:mute" => {
            Command::new("osascript")
                .args(["-e", "set volume with output muted"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:unmute" => {
            Command::new("osascript")
                .args(["-e", "set volume without output muted"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown command: {}", command_id)),
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    match command_id {
        "cmd:lock" => {
            Command::new("rundll32.exe")
                .args(["user32.dll,LockWorkStation"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:sleep" => {
            Command::new("rundll32.exe")
                .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:restart" => {
            Command::new("shutdown")
                .args(["/r", "/t", "0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:shutdown" => {
            Command::new("shutdown")
                .args(["/s", "/t", "0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:logout" => {
            Command::new("shutdown")
                .args(["/l"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:emptytrash" => {
            Command::new("powershell")
                .args(["-Command", "Clear-RecycleBin -Force"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:mute" => {
            Command::new("powershell")
                .args(["-Command", "(New-Object -ComObject WScript.Shell).SendKeys([char]173)"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:unmute" => {
            Command::new("powershell")
                .args(["-Command", "(New-Object -ComObject WScript.Shell).SendKeys([char]173)"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown command: {}", command_id)),
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    Err(format!("System commands not supported on this platform: {}", command_id))
}
```

**Step 3: Create actions module**

Create `src-tauri/src/actions/mod.rs`:
```rust
pub mod system;

use std::process::Command;

#[cfg(target_os = "macos")]
pub fn launch_app(path: &str) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn launch_app(path: &str) -> Result<(), String> {
    Command::new("cmd")
        .args(["/C", "start", "", path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn launch_app(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn open_url(url: &str) -> Result<(), String> {
    open::that(url).map_err(|e| e.to_string())
}
```

**Step 4: Add open dependency**

Add to `src-tauri/Cargo.toml` dependencies:
```toml
open = "5.0"
```

**Step 5: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add actions module with system commands"
```

---

### Task 11: Integrate Actions and System Commands with Search

**Files:**
- Modify: `src-tauri/src/main.rs`

**Step 1: Update main.rs to include system commands in search**

Update `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod config;
mod db;
mod indexers;
mod search;

use actions::system::{execute_command, get_system_commands};
use config::settings::Settings;
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
use search::{ResultType, SearchAction, SearchEngine, SearchResult};
use std::sync::{Arc, RwLock};
use tauri::State;

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings_cmd(settings: Settings, state: State<AppState>) -> Result<(), String> {
    config::save_settings(&settings)?;
    *state.settings.write().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn reindex_apps(state: State<AppState>) -> usize {
    let indexer = get_indexer();
    let apps = indexer.index_apps();
    let count = apps.len();
    *state.indexed_apps.write().unwrap() = apps;
    count
}

#[tauri::command]
fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    if query.is_empty() {
        return vec![];
    }

    let settings = state.settings.read().unwrap();
    let apps = state.indexed_apps.read().unwrap();

    let mut items: Vec<SearchResult> = Vec::new();

    // Check for web search keyword match first
    for ws in &settings.web_searches {
        if query.starts_with(&format!("{} ", ws.keyword)) {
            let search_query = query.strip_prefix(&format!("{} ", ws.keyword)).unwrap();
            if !search_query.is_empty() {
                let url = if ws.requires_setup && ws.instance.is_none() {
                    continue; // Skip if requires setup but not configured
                } else if ws.requires_setup {
                    ws.url
                        .replace("{instance}", ws.instance.as_ref().unwrap())
                        .replace("{query}", &urlencoding::encode(search_query))
                } else {
                    ws.url.replace("{query}", &urlencoding::encode(search_query))
                };

                items.push(SearchResult {
                    id: format!("web:{}", ws.keyword),
                    name: format!("{}: {}", ws.name, search_query),
                    description: "Web Search".to_string(),
                    icon: ws.icon.clone(),
                    result_type: ResultType::WebSearch,
                    score: 10000, // High score to appear first
                    action: SearchAction::OpenUrl { url },
                });
            }
        }
    }

    // Check for system command prefix
    let is_command_query = query.starts_with('>');
    let command_query = if is_command_query {
        query.strip_prefix('>').unwrap_or(&query).trim()
    } else {
        &query
    };

    // Add system commands
    for cmd in get_system_commands() {
        let matches = cmd.aliases.iter().any(|alias| {
            alias.to_lowercase().contains(&command_query.to_lowercase())
        });

        if matches || is_command_query {
            items.push(SearchResult {
                id: cmd.id.clone(),
                name: cmd.name.clone(),
                description: cmd.description.clone(),
                icon: Some("system".to_string()),
                result_type: ResultType::SystemCommand,
                score: if is_command_query { 5000 } else { 0 },
                action: SearchAction::RunCommand { command: cmd.id },
            });
        }
    }

    // Add apps (skip if web search or command prefix)
    if !query.contains(' ') || (!is_command_query && items.is_empty()) {
        for app in apps.iter() {
            items.push(SearchResult {
                id: app.id.clone(),
                name: app.name.clone(),
                description: "Application".to_string(),
                icon: app.icon_cache_path.clone(),
                result_type: ResultType::Application,
                score: 0,
                action: SearchAction::LaunchApp {
                    path: app.path.clone(),
                },
            });
        }
    }

    // Search and limit results
    let mut results = if is_command_query {
        state.search_engine.search(command_query, items)
    } else {
        state.search_engine.search(&query, items)
    };

    results.truncate(settings.search.max_results);
    results
}

#[tauri::command]
fn execute_action(action: SearchAction) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => actions::launch_app(&path),
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
    }
}

fn main() {
    let db = Database::new().expect("Failed to initialize database");
    let settings = config::load_settings();
    let indexer = get_indexer();
    let indexed_apps = indexer.index_apps();

    tauri::Builder::default()
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
            indexed_apps: RwLock::new(indexed_apps),
            settings: RwLock::new(settings),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings_cmd,
            reindex_apps,
            search,
            execute_action
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 2: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: integrate system commands with search and actions"
```

---

## Phase 5: Frontend UI

### Task 12: Create Zustand Store

**Files:**
- Create: `src/stores/app.ts`
- Create: `src/types.ts`

**Step 1: Create types file**

Create `src/types.ts`:
```typescript
export interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  result_type: 'application' | 'web_search' | 'system_command';
  score: number;
  action: SearchAction;
}

export type SearchAction =
  | { type: 'launch_app'; path: string }
  | { type: 'open_url'; url: string }
  | { type: 'run_command'; command: string };

export interface Settings {
  general: GeneralSettings;
  activation: ActivationSettings;
  search: SearchSettings;
  theme: ThemeSettings;
  web_searches: WebSearch[];
}

export interface GeneralSettings {
  launch_at_login: boolean;
  show_in_dock: boolean;
  show_in_taskbar: boolean;
}

export interface ActivationSettings {
  hotkey: string;
  show_tray_icon: boolean;
}

export interface SearchSettings {
  max_results: number;
  show_recently_used: boolean;
  fuzzy_match_threshold: number;
}

export interface ThemeSettings {
  mode: 'light' | 'dark' | 'system';
  accent_color: string;
  custom?: CustomTheme;
}

export interface CustomTheme {
  background?: string;
  foreground?: string;
  border?: string;
  selected_background?: string;
  input_background?: string;
  font_family?: string;
  font_size?: number;
  border_radius?: number;
}

export interface WebSearch {
  name: string;
  keyword: string;
  url: string;
  icon?: string;
  requires_setup: boolean;
  instance?: string;
}
```

**Step 2: Create stores directory**

Run:
```bash
mkdir -p src/stores
```

**Step 3: Create app store**

Create `src/stores/app.ts`:
```typescript
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { SearchResult, Settings } from '../types';

interface AppState {
  query: string;
  results: SearchResult[];
  selectedIndex: number;
  settings: Settings | null;
  isLoading: boolean;

  setQuery: (query: string) => void;
  setSelectedIndex: (index: number) => void;
  moveSelection: (delta: number) => void;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: Settings) => Promise<void>;
  executeSelected: () => Promise<void>;
  reindexApps: () => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  query: '',
  results: [],
  selectedIndex: 0,
  settings: null,
  isLoading: false,

  setQuery: async (query: string) => {
    set({ query, selectedIndex: 0 });

    if (!query.trim()) {
      set({ results: [] });
      return;
    }

    try {
      const results = await invoke<SearchResult[]>('search', { query });
      set({ results });
    } catch (error) {
      console.error('Search error:', error);
      set({ results: [] });
    }
  },

  setSelectedIndex: (index: number) => {
    const { results } = get();
    if (index >= 0 && index < results.length) {
      set({ selectedIndex: index });
    }
  },

  moveSelection: (delta: number) => {
    const { selectedIndex, results } = get();
    const newIndex = Math.max(0, Math.min(results.length - 1, selectedIndex + delta));
    set({ selectedIndex: newIndex });
  },

  loadSettings: async () => {
    try {
      const settings = await invoke<Settings>('get_settings');
      set({ settings });
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  },

  saveSettings: async (settings: Settings) => {
    try {
      await invoke('save_settings_cmd', { settings });
      set({ settings });
    } catch (error) {
      console.error('Failed to save settings:', error);
    }
  },

  executeSelected: async () => {
    const { results, selectedIndex } = get();
    const selected = results[selectedIndex];

    if (!selected) return;

    try {
      await invoke('execute_action', { action: selected.action });
      set({ query: '', results: [], selectedIndex: 0 });
    } catch (error) {
      console.error('Failed to execute action:', error);
    }
  },

  reindexApps: async () => {
    set({ isLoading: true });
    try {
      await invoke('reindex_apps');
    } catch (error) {
      console.error('Failed to reindex:', error);
    }
    set({ isLoading: false });
  },
}));
```

**Step 4: Verify TypeScript compiles**

Run:
```bash
npm run build
```

Expected: Build succeeds (may have warnings)

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add Zustand store and TypeScript types"
```

---

### Task 13: Create Search Bar Component

**Files:**
- Create: `src/components/SearchBar.tsx`

**Step 1: Create components directory**

Run:
```bash
mkdir -p src/components
```

**Step 2: Create SearchBar component**

Create `src/components/SearchBar.tsx`:
```tsx
import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';

export function SearchBar() {
  const inputRef = useRef<HTMLInputElement>(null);
  const { query, setQuery, moveSelection, executeSelected } = useAppStore();

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        moveSelection(1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        moveSelection(-1);
        break;
      case 'Enter':
        e.preventDefault();
        executeSelected();
        break;
      case 'Escape':
        e.preventDefault();
        setQuery('');
        break;
    }
  };

  return (
    <div className="p-3 border-b border-[var(--border)]">
      <input
        ref={inputRef}
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search apps, commands, or type a keyword..."
        className="w-full px-4 py-2 text-lg bg-[var(--input-bg)] text-[var(--foreground)] rounded-lg outline-none focus:ring-2 focus:ring-blue-500"
        autoFocus
      />
    </div>
  );
}
```

**Step 3: Verify TypeScript compiles**

Run:
```bash
npm run build
```

Expected: Build succeeds

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add SearchBar component"
```

---

### Task 14: Create Result Item Component

**Files:**
- Create: `src/components/ResultItem.tsx`

**Step 1: Create ResultItem component**

Create `src/components/ResultItem.tsx`:
```tsx
import type { SearchResult } from '../types';

interface ResultItemProps {
  result: SearchResult;
  isSelected: boolean;
  onClick: () => void;
}

export function ResultItem({ result, isSelected, onClick }: ResultItemProps) {
  const getIcon = () => {
    switch (result.result_type) {
      case 'application':
        return '📱';
      case 'web_search':
        return '🌐';
      case 'system_command':
        return '⚙️';
      default:
        return '📄';
    }
  };

  const getTypeLabel = () => {
    switch (result.result_type) {
      case 'application':
        return 'Application';
      case 'web_search':
        return 'Web Search';
      case 'system_command':
        return 'Command';
      default:
        return 'Item';
    }
  };

  return (
    <div
      onClick={onClick}
      className={`flex items-center px-4 py-3 cursor-pointer transition-colors ${
        isSelected
          ? 'bg-[var(--selected)]'
          : 'hover:bg-[var(--selected)] hover:bg-opacity-50'
      }`}
    >
      <span className="text-2xl mr-3">{getIcon()}</span>
      <div className="flex-1 min-w-0">
        <div className="font-medium truncate">{result.name}</div>
        <div className="text-sm text-gray-500 truncate">{result.description}</div>
      </div>
      <div className="text-xs text-gray-400 ml-2">{getTypeLabel()}</div>
    </div>
  );
}
```

**Step 2: Verify TypeScript compiles**

Run:
```bash
npm run build
```

Expected: Build succeeds

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add ResultItem component"
```

---

### Task 15: Create Results List Component

**Files:**
- Create: `src/components/ResultsList.tsx`

**Step 1: Create ResultsList component**

Create `src/components/ResultsList.tsx`:
```tsx
import { useAppStore } from '../stores/app';
import { ResultItem } from './ResultItem';

export function ResultsList() {
  const { results, selectedIndex, setSelectedIndex, executeSelected } = useAppStore();

  if (results.length === 0) {
    return null;
  }

  return (
    <div className="max-h-[300px] overflow-y-auto">
      {results.map((result, index) => (
        <ResultItem
          key={result.id}
          result={result}
          isSelected={index === selectedIndex}
          onClick={() => {
            setSelectedIndex(index);
            executeSelected();
          }}
        />
      ))}
    </div>
  );
}
```

**Step 2: Verify TypeScript compiles**

Run:
```bash
npm run build
```

Expected: Build succeeds

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add ResultsList component"
```

---

### Task 16: Assemble Main App Component

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/main.tsx`

**Step 1: Update App.tsx**

Replace `src/App.tsx`:
```tsx
import { useEffect } from 'react';
import { SearchBar } from './components/SearchBar';
import { ResultsList } from './components/ResultsList';
import { useAppStore } from './stores/app';

function App() {
  const { loadSettings, reindexApps, settings } = useAppStore();

  useEffect(() => {
    loadSettings();
    reindexApps();
  }, []);

  // Apply theme
  useEffect(() => {
    if (!settings) return;

    const { mode } = settings.theme;
    const root = document.documentElement;

    if (mode === 'system') {
      const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
      root.classList.toggle('dark', isDark);
    } else {
      root.classList.toggle('dark', mode === 'dark');
    }
  }, [settings?.theme.mode]);

  return (
    <div className="min-h-screen bg-[var(--background)] text-[var(--foreground)] rounded-lg overflow-hidden border border-[var(--border)]">
      <SearchBar />
      <ResultsList />
    </div>
  );
}

export default App;
```

**Step 2: Verify the app builds**

Run:
```bash
npm run build
```

Expected: Build succeeds

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: assemble main App component with search UI"
```

---

### Task 16.5: Test Phase 5 - Frontend UI

**Files:**
- Create: `src/__tests__/stores/app.test.ts`
- Create: `src/__tests__/components/ResultItem.test.tsx`
- Modify: `package.json`

**Step 1: Install testing dependencies**

Run:
```bash
npm install -D vitest @testing-library/react @testing-library/jest-dom jsdom @types/node
```

**Step 2: Create vitest config**

Create `vitest.config.ts`:
```typescript
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/__tests__/setup.ts'],
  },
});
```

**Step 3: Create test setup file**

Create `src/__tests__/setup.ts`:
```typescript
import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
```

**Step 4: Create store tests**

Create `src/__tests__/stores/app.test.ts`:
```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../stores/app';

vi.mock('@tauri-apps/api/core');

describe('AppStore', () => {
  beforeEach(() => {
    // Reset store state
    useAppStore.setState({
      query: '',
      results: [],
      selectedIndex: 0,
      settings: null,
      isLoading: false,
    });
  });

  it('should initialize with empty state', () => {
    const state = useAppStore.getState();
    expect(state.query).toBe('');
    expect(state.results).toEqual([]);
    expect(state.selectedIndex).toBe(0);
  });

  it('should move selection within bounds', () => {
    useAppStore.setState({
      results: [
        { id: '1', name: 'App1', description: '', icon: null, result_type: 'application', score: 0, action: { type: 'launch_app', path: '' } },
        { id: '2', name: 'App2', description: '', icon: null, result_type: 'application', score: 0, action: { type: 'launch_app', path: '' } },
      ],
      selectedIndex: 0,
    });

    const { moveSelection } = useAppStore.getState();

    moveSelection(1);
    expect(useAppStore.getState().selectedIndex).toBe(1);

    moveSelection(1);
    expect(useAppStore.getState().selectedIndex).toBe(1); // Should not exceed bounds

    moveSelection(-2);
    expect(useAppStore.getState().selectedIndex).toBe(0); // Should not go below 0
  });

  it('should call search API when query changes', async () => {
    vi.mocked(invoke).mockResolvedValue([]);

    const { setQuery } = useAppStore.getState();
    await setQuery('test');

    expect(invoke).toHaveBeenCalledWith('search', { query: 'test' });
  });
});
```

**Step 5: Create component tests**

Create directory and file:
```bash
mkdir -p src/__tests__/stores src/__tests__/components
```

Create `src/__tests__/components/ResultItem.test.tsx`:
```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ResultItem } from '../../components/ResultItem';

describe('ResultItem', () => {
  const mockResult = {
    id: '1',
    name: 'Test App',
    description: 'A test application',
    icon: null,
    result_type: 'application' as const,
    score: 100,
    action: { type: 'launch_app' as const, path: '/test' },
  };

  it('should render result name and description', () => {
    render(<ResultItem result={mockResult} isSelected={false} onClick={() => {}} />);

    expect(screen.getByText('Test App')).toBeInTheDocument();
    expect(screen.getByText('A test application')).toBeInTheDocument();
  });

  it('should show type label', () => {
    render(<ResultItem result={mockResult} isSelected={false} onClick={() => {}} />);

    expect(screen.getByText('Application')).toBeInTheDocument();
  });

  it('should apply selected styles when selected', () => {
    const { container } = render(
      <ResultItem result={mockResult} isSelected={true} onClick={() => {}} />
    );

    expect(container.firstChild).toHaveClass('bg-[var(--selected)]');
  });

  it('should call onClick when clicked', () => {
    const onClick = vi.fn();
    render(<ResultItem result={mockResult} isSelected={false} onClick={onClick} />);

    fireEvent.click(screen.getByText('Test App'));
    expect(onClick).toHaveBeenCalled();
  });

  it('should show correct icon for web search', () => {
    const webResult = {
      ...mockResult,
      result_type: 'web_search' as const,
      action: { type: 'open_url' as const, url: 'https://test.com' },
    };

    render(<ResultItem result={webResult} isSelected={false} onClick={() => {}} />);
    expect(screen.getByText('Web Search')).toBeInTheDocument();
  });
});
```

**Step 6: Add test script to package.json**

Add to scripts section in `package.json`:
```json
{
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest"
  }
}
```

**Step 7: Run frontend tests**

Run:
```bash
npm test
```

Expected: All tests pass

**Step 8: Run full build to verify everything works**

Run:
```bash
npm run build && cd src-tauri && cargo test && cd ..
```

Expected: Both builds and tests pass

**Step 9: Commit**

```bash
git add -A
git commit -m "test: add frontend unit tests for store and components"
```

---

## Phase 6: Window Management & Hotkeys

### Task 17: Add Global Hotkey Support

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/main.rs`

**Step 1: Add global-hotkey dependency**

This requires using Tauri plugins. Update `src-tauri/Cargo.toml`:
```toml
[dependencies]
# ... existing deps ...
tauri-plugin-global-shortcut = "2"
```

Also update `src-tauri/tauri.conf.json` to add the plugin capability. After reading the file, add to the plugins section:
```json
{
  "plugins": {
    "global-shortcut": {}
  }
}
```

**Step 2: Update main.rs for hotkey handling**

Update `src-tauri/src/main.rs` to add hotkey registration:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod config;
mod db;
mod indexers;
mod search;

use actions::system::{execute_command, get_system_commands};
use config::settings::Settings;
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
use search::{ResultType, SearchAction, SearchEngine, SearchResult};
use std::sync::{Arc, RwLock};
use tauri::{Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings_cmd(settings: Settings, state: State<AppState>) -> Result<(), String> {
    config::save_settings(&settings)?;
    *state.settings.write().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn reindex_apps(state: State<AppState>) -> usize {
    let indexer = get_indexer();
    let apps = indexer.index_apps();
    let count = apps.len();
    *state.indexed_apps.write().unwrap() = apps;
    count
}

#[tauri::command]
fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    if query.is_empty() {
        return vec![];
    }

    let settings = state.settings.read().unwrap();
    let apps = state.indexed_apps.read().unwrap();

    let mut items: Vec<SearchResult> = Vec::new();

    // Check for web search keyword match first
    for ws in &settings.web_searches {
        if query.starts_with(&format!("{} ", ws.keyword)) {
            let search_query = query.strip_prefix(&format!("{} ", ws.keyword)).unwrap();
            if !search_query.is_empty() {
                let url = if ws.requires_setup && ws.instance.is_none() {
                    continue;
                } else if ws.requires_setup {
                    ws.url
                        .replace("{instance}", ws.instance.as_ref().unwrap())
                        .replace("{query}", &urlencoding::encode(search_query))
                } else {
                    ws.url.replace("{query}", &urlencoding::encode(search_query))
                };

                items.push(SearchResult {
                    id: format!("web:{}", ws.keyword),
                    name: format!("{}: {}", ws.name, search_query),
                    description: "Web Search".to_string(),
                    icon: ws.icon.clone(),
                    result_type: ResultType::WebSearch,
                    score: 10000,
                    action: SearchAction::OpenUrl { url },
                });
            }
        }
    }

    // Check for system command prefix
    let is_command_query = query.starts_with('>');
    let command_query = if is_command_query {
        query.strip_prefix('>').unwrap_or(&query).trim()
    } else {
        &query
    };

    // Add system commands
    for cmd in get_system_commands() {
        let matches = cmd.aliases.iter().any(|alias| {
            alias.to_lowercase().contains(&command_query.to_lowercase())
        });

        if matches || is_command_query {
            items.push(SearchResult {
                id: cmd.id.clone(),
                name: cmd.name.clone(),
                description: cmd.description.clone(),
                icon: Some("system".to_string()),
                result_type: ResultType::SystemCommand,
                score: if is_command_query { 5000 } else { 0 },
                action: SearchAction::RunCommand { command: cmd.id },
            });
        }
    }

    // Add apps
    if !query.contains(' ') || (!is_command_query && items.is_empty()) {
        for app in apps.iter() {
            items.push(SearchResult {
                id: app.id.clone(),
                name: app.name.clone(),
                description: "Application".to_string(),
                icon: app.icon_cache_path.clone(),
                result_type: ResultType::Application,
                score: 0,
                action: SearchAction::LaunchApp {
                    path: app.path.clone(),
                },
            });
        }
    }

    let mut results = if is_command_query {
        state.search_engine.search(command_query, items)
    } else {
        state.search_engine.search(&query, items)
    };

    results.truncate(settings.search.max_results);
    results
}

#[tauri::command]
fn execute_action(action: SearchAction) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => actions::launch_app(&path),
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
    }
}

#[tauri::command]
fn hide_window(window: tauri::Window) {
    window.hide().ok();
}

#[tauri::command]
fn show_window(window: tauri::Window) {
    window.show().ok();
    window.set_focus().ok();
}

fn main() {
    let db = Database::new().expect("Failed to initialize database");
    let settings = config::load_settings();
    let indexer = get_indexer();
    let indexed_apps = indexer.index_apps();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
            indexed_apps: RwLock::new(indexed_apps),
            settings: RwLock::new(settings),
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Register global shortcut (Alt+Space)
            let shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);

            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    if window.is_visible().unwrap_or(false) {
                        window.hide().ok();
                    } else {
                        window.show().ok();
                        window.set_focus().ok();
                    }
                }
            })?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings_cmd,
            reindex_apps,
            search,
            execute_action,
            hide_window,
            show_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 3: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add global hotkey support for launcher toggle"
```

---

### Task 18: Handle Window Dismiss on Action

**Files:**
- Modify: `src/stores/app.ts`

**Step 1: Update store to hide window after action**

Update `src/stores/app.ts`:
```typescript
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type { SearchResult, Settings } from '../types';

interface AppState {
  query: string;
  results: SearchResult[];
  selectedIndex: number;
  settings: Settings | null;
  isLoading: boolean;

  setQuery: (query: string) => void;
  setSelectedIndex: (index: number) => void;
  moveSelection: (delta: number) => void;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: Settings) => Promise<void>;
  executeSelected: () => Promise<void>;
  reindexApps: () => Promise<void>;
  hideWindow: () => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  query: '',
  results: [],
  selectedIndex: 0,
  settings: null,
  isLoading: false,

  setQuery: async (query: string) => {
    set({ query, selectedIndex: 0 });

    if (!query.trim()) {
      set({ results: [] });
      return;
    }

    try {
      const results = await invoke<SearchResult[]>('search', { query });
      set({ results });
    } catch (error) {
      console.error('Search error:', error);
      set({ results: [] });
    }
  },

  setSelectedIndex: (index: number) => {
    const { results } = get();
    if (index >= 0 && index < results.length) {
      set({ selectedIndex: index });
    }
  },

  moveSelection: (delta: number) => {
    const { selectedIndex, results } = get();
    const newIndex = Math.max(0, Math.min(results.length - 1, selectedIndex + delta));
    set({ selectedIndex: newIndex });
  },

  loadSettings: async () => {
    try {
      const settings = await invoke<Settings>('get_settings');
      set({ settings });
    } catch (error) {
      console.error('Failed to load settings:', error);
    }
  },

  saveSettings: async (settings: Settings) => {
    try {
      await invoke('save_settings_cmd', { settings });
      set({ settings });
    } catch (error) {
      console.error('Failed to save settings:', error);
    }
  },

  executeSelected: async () => {
    const { results, selectedIndex, hideWindow } = get();
    const selected = results[selectedIndex];

    if (!selected) return;

    try {
      await invoke('execute_action', { action: selected.action });
      set({ query: '', results: [], selectedIndex: 0 });
      await hideWindow();
    } catch (error) {
      console.error('Failed to execute action:', error);
    }
  },

  reindexApps: async () => {
    set({ isLoading: true });
    try {
      await invoke('reindex_apps');
    } catch (error) {
      console.error('Failed to reindex:', error);
    }
    set({ isLoading: false });
  },

  hideWindow: async () => {
    try {
      await invoke('hide_window');
    } catch (error) {
      console.error('Failed to hide window:', error);
    }
  },
}));
```

**Step 2: Update SearchBar to handle Escape**

Update `src/components/SearchBar.tsx`:
```tsx
import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';

export function SearchBar() {
  const inputRef = useRef<HTMLInputElement>(null);
  const { query, setQuery, moveSelection, executeSelected, hideWindow } = useAppStore();

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        moveSelection(1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        moveSelection(-1);
        break;
      case 'Enter':
        e.preventDefault();
        executeSelected();
        break;
      case 'Escape':
        e.preventDefault();
        if (query) {
          setQuery('');
        } else {
          hideWindow();
        }
        break;
    }
  };

  return (
    <div className="p-3 border-b border-[var(--border)]">
      <input
        ref={inputRef}
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search apps, commands, or type a keyword..."
        className="w-full px-4 py-2 text-lg bg-[var(--input-bg)] text-[var(--foreground)] rounded-lg outline-none focus:ring-2 focus:ring-blue-500"
        autoFocus
      />
    </div>
  );
}
```

**Step 3: Verify TypeScript compiles**

Run:
```bash
npm run build
```

Expected: Build succeeds

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: hide window on action execution and escape key"
```

---

## Phase 7: Final Integration & Testing

### Task 19: Test Full Application Flow

**Step 1: Run the development server**

Run:
```bash
npm run tauri dev
```

Expected: Application window appears (may be hidden initially)

**Step 2: Test hotkey activation**

Press `Alt+Space` to toggle the window.

Expected: Window appears/disappears

**Step 3: Test app search**

Type part of an application name.

Expected: Matching apps appear in results

**Step 4: Test web search**

Type `g hello world`.

Expected: "Google: hello world" appears as top result

**Step 5: Test system command**

Type `>lock`.

Expected: "Lock" command appears in results

**Step 6: Test action execution**

Select a web search result and press Enter.

Expected: Browser opens with search, window hides

**Step 7: Commit any fixes**

```bash
git add -A
git commit -m "test: verify full application flow"
```

---

### Task 20: Add System Tray Support

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/main.rs`

**Step 1: Add tray plugin**

Add to `src-tauri/Cargo.toml`:
```toml
tauri-plugin-shell = "2"
```

**Step 2: Update main.rs with tray**

The tray implementation depends on Tauri 2.x APIs. Update the setup function in `src-tauri/src/main.rs`:
```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager, State,
};

// In the setup function, after registering the shortcut:
let quit = MenuItem::with_id(app, "quit", "Quit Watson", true, None::<&str>)?;
let show = MenuItem::with_id(app, "show", "Show Watson", true, None::<&str>)?;

let menu = Menu::with_items(app, &[&show, &quit])?;

let _tray = TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .menu(&menu)
    .on_menu_event(|app, event| match event.id.as_ref() {
        "quit" => {
            app.exit(0);
        }
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                window.show().ok();
                window.set_focus().ok();
            }
        }
        _ => {}
    })
    .build(app)?;
```

**Step 3: Verify it compiles**

Run:
```bash
cd src-tauri && cargo check && cd ..
```

Expected: No errors

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add system tray with menu"
```

---

### Task 21: Final Build Test

**Step 1: Build production version**

Run:
```bash
npm run tauri build
```

Expected: Build completes, installer created in `src-tauri/target/release/bundle/`

**Step 2: Verify installer exists**

Run:
```bash
ls -la src-tauri/target/release/bundle/
```

Expected: Platform-specific installer files present

**Step 3: Final commit**

```bash
git add -A
git commit -m "build: verify production build succeeds"
```

---

## Summary

This implementation plan creates a functional Watson launcher with:

1. **Core Infrastructure**: Tauri + React + TypeScript project scaffold
2. **Configuration**: TOML-based settings with defaults
3. **Database**: SQLite for app index and history
4. **Search**: Fuzzy matching with frecency scoring
5. **Indexers**: Platform-specific app discovery (macOS + Windows)
6. **Actions**: App launch, URL open, system commands
7. **UI**: Minimal launcher interface with keyboard navigation
8. **Window Management**: Global hotkey toggle, auto-hide
9. **System Tray**: Background presence with menu

Future enhancements (post-v1):
- Clipboard history
- Snippets/text expansion
- File search
- Calculator
- Workflows/plugins
- Custom themes UI
