# Watson: Cross-Platform Productivity Launcher

## Overview

Watson is a cross-platform productivity launcher for macOS and Windows, inspired by Alfred. It provides keyboard-first access to applications, web searches, and system commands.

## Goals

- **Speed**: Keyboard-first access to everything
- **Unified search**: One place to search apps, web, and commands
- **Automation-ready**: Architecture supports future workflows/plugins
- **Cross-platform parity**: Same experience on Mac and Windows
- **Production-quality**: Distribution-ready from v1

## V1 Features

1. **App launcher** - Search and launch installed applications
2. **Web search** - Keyword-based web searches (configurable styles)
3. **System commands** - Sleep, lock, empty trash, etc.

## Technology Stack

| Layer | Technology | Rationale |
|-------|------------|-----------|
| Backend | Rust (Tauri) | Fast, safe, small footprint (~10MB vs Electron's 150MB+) |
| Frontend | React + TypeScript | Large ecosystem, familiar |
| Styling | Tailwind CSS + CSS variables | Rapid development, easy theming |
| State | Zustand | Lightweight, TypeScript-friendly |
| Build | Vite | Fast development, Tauri integration |
| Database | SQLite | Structured storage for history, index cache |
| Config | TOML | Human-readable settings file |

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Frontend (React)                 │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────┐  │
│  │ Search Bar  │ │ Results List│ │ Settings UI  │  │
│  └─────────────┘ └─────────────┘ └──────────────┘  │
└─────────────────────────────────────────────────────┘
                         │ Tauri IPC
┌─────────────────────────────────────────────────────┐
│                   Backend (Rust)                    │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────┐  │
│  │  Indexers   │ │Search Engine│ │Action Executor│ │
│  └─────────────┘ └─────────────┘ └──────────────┘  │
│  ┌─────────────┐ ┌─────────────┐ ┌──────────────┐  │
│  │Config Manager│ │ Data Store │ │ Hotkey Mgr   │  │
│  └─────────────┘ └─────────────┘ └──────────────┘  │
└─────────────────────────────────────────────────────┘
```

### Components

- **Indexers**: Discover and catalog apps. Each indexer is a trait implementation (extensibility point).
- **Search Engine**: Fuzzy matching, ranking, filtering. Operates on indexed items.
- **Action Executor**: Launches apps, opens URLs, runs system commands. Each action type is a trait implementation.
- **Config Manager**: Reads/writes TOML settings file, exposes to frontend.
- **Data Store**: SQLite wrapper for search history, index cache.
- **Hotkey Manager**: Registers global hotkeys, handles activation.

## App Launcher

### Application Discovery

**macOS:**
- Scan `/Applications`, `~/Applications`, `/System/Applications`
- Parse `.app` bundles for metadata (name, icon, bundle ID)
- Watch directories for changes (new installs/uninstalls)

**Windows:**
- Read Start Menu shortcuts (`%ProgramData%\Microsoft\Windows\Start Menu`, `%AppData%\Microsoft\Windows\Start Menu`)
- Query installed apps from registry (`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`)
- Extract icons from `.exe` and `.lnk` files

### Search Ranking

1. **Fuzzy match score** - How well the query matches the app name
2. **Frecency** - Combination of frequency + recency of launches
3. **Exact prefix match boost** - Typing "chr" prioritizes "Chrome"

### Launch Tracking

Increment `launch_count` and update `last_launched` on every launch. This data improves ranking over time.

## Web Search

### Configuration

```toml
[[web_searches]]
name = "Google"
keyword = "g"
url = "https://www.google.com/search?q={query}"
icon = "google"

[[web_searches]]
name = "Jira"
keyword = "jira"
url = "https://{instance}.atlassian.net/browse/{query}"
icon = "jira"
requires_setup = true
```

### Search Modes (all configurable)

1. **Prefix keywords** - `g how to cook pasta` → Google search
2. **Domain shortcuts** - `google.com pasta recipes` → Google search
3. **Bang syntax** - `!yt music video` (DuckDuckGo-style)
4. **URL detection** - Typing a URL opens it directly

### Default Searches

Google, DuckDuckGo, YouTube, GitHub, Wikipedia, Stack Overflow, Jira (requires setup)

## System Commands

| Command | macOS | Windows |
|---------|-------|---------|
| `lock` | Lock screen | Lock workstation |
| `sleep` | Sleep system | Sleep system |
| `restart` | Restart | Restart |
| `shutdown` | Shut down | Shut down |
| `logout` | Log out current user | Sign out |
| `trash` / `emptytrash` | Empty Trash | Empty Recycle Bin |
| `screensaver` | Start screensaver | Start screensaver |
| `mute` / `unmute` | Toggle system audio | Toggle system audio |
| `eject` | Eject mounted volumes | Eject removable drives |

Commands are prefixed with `>` (e.g., `>lock`) or shown as secondary results. Destructive commands show confirmation dialogs.

## UI & Theming

### Window Behavior

- Borderless floating window, centered on active screen
- Appears/dismisses instantly on hotkey
- Dismisses on: Escape key, click outside, launching an action
- Remembers position if user drags it

### Layout

```
┌────────────────────────────────────────┐
│  🔍  [Search input....................]│
├────────────────────────────────────────┤
│  📱  Chrome              Application   │
│  📱  Chromium            Application   │ ← Selected
│  🌐  g: Search Google    Web Search    │
│  ⚙️  >lock               Command       │
└────────────────────────────────────────┘
```

### Theme Configuration

```toml
[theme]
mode = "system"  # "light", "dark", or "system"
accent_color = "system"  # or hex value

[theme.custom]
background = "#1a1a2e"
foreground = "#eaeaea"
border = "#333355"
selected_background = "#4a4a6a"
input_background = "#252540"
font_family = "SF Pro, Segoe UI, system-ui"
font_size = 14
border_radius = 8
```

## Settings

### File Location

- macOS: `~/Library/Application Support/Watson/config.toml`
- Windows: `%APPDATA%\Watson\config.toml`

### Structure

```toml
[general]
launch_at_login = true
show_in_dock = false
show_in_taskbar = false

[activation]
hotkey = "Alt+Space"
show_tray_icon = true
double_tap_modifier = false

[search]
max_results = 8
show_recently_used = true
fuzzy_match_threshold = 0.6

[updates]
check_automatically = true
update_mode = "prompt"  # "silent", "prompt", "manual"

[theme]
# ... theme settings

[[web_searches]]
# ... web search definitions
```

### Settings UI Tabs

- General
- Hotkey & Activation
- Search
- Web Searches
- Appearance
- Updates
- About

## Data Storage (SQLite)

### Location

- macOS: `~/Library/Application Support/Watson/watson.db`
- Windows: `%APPDATA%\Watson\watson.db`

### Schema

```sql
CREATE TABLE apps (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL,
  icon_cache_path TEXT,
  launch_count INTEGER DEFAULT 0,
  last_launched INTEGER,
  platform TEXT NOT NULL,
  indexed_at INTEGER NOT NULL
);

CREATE TABLE search_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  query TEXT NOT NULL,
  selected_item_id TEXT,
  selected_item_type TEXT,
  timestamp INTEGER NOT NULL
);

CREATE TABLE icons (
  id TEXT PRIMARY KEY,
  source_path TEXT NOT NULL,
  cache_path TEXT NOT NULL,
  hash TEXT NOT NULL,
  cached_at INTEGER NOT NULL
);

CREATE INDEX idx_apps_name ON apps(name);
CREATE INDEX idx_history_query ON search_history(query);
CREATE INDEX idx_history_timestamp ON search_history(timestamp);
```

### Maintenance

- Prune search history older than 90 days
- Re-index apps on startup and on file system changes
- Rebuild icon cache if source file hash changes

## Build & Distribution

### Build Pipeline

```
Push to main → Build for all targets → Sign & Notarize → Create release
```

### Build Targets

- macOS: Universal binary (Intel + Apple Silicon), `.dmg` installer
- Windows: x64, `.msi` installer + portable `.exe`

### Code Signing

- macOS: Apple Developer certificate, notarization via `notarytool`
- Windows: Code signing certificate (DigiCert, Sectigo, etc.)

### Distribution Channels

| Channel | macOS | Windows |
|---------|-------|---------|
| Direct download | GitHub Releases + website | GitHub Releases + website |
| App Store | Mac App Store | Microsoft Store |
| Package manager | Homebrew cask | Winget, Chocolatey |

### Auto-Updates

Uses Tauri's built-in updater:
- Checks update server (GitHub Releases or custom endpoint)
- Downloads update in background
- Applies based on `update_mode` setting

## Project Structure

```
watson/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands/
│   │   ├── indexers/
│   │   │   ├── mod.rs
│   │   │   ├── macos.rs
│   │   │   └── windows.rs
│   │   ├── search/
│   │   ├── actions/
│   │   ├── config/
│   │   ├── db/
│   │   └── hotkey/
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/
│   ├── components/
│   │   ├── SearchBar.tsx
│   │   ├── ResultsList.tsx
│   │   ├── ResultItem.tsx
│   │   └── Settings/
│   ├── hooks/
│   ├── stores/
│   ├── styles/
│   ├── App.tsx
│   └── main.tsx
├── package.json
├── vite.config.ts
├── tailwind.config.js
└── docs/
    └── plans/
```

## Testing Strategy

- **Rust**: Unit tests for indexers, search, actions (`cargo test`)
- **Frontend**: Vitest for component/logic tests
- **E2E**: Playwright or WebdriverIO for full app testing

## Future Features (Post-V1)

- Clipboard history
- Snippet/text expansion
- File search
- Calculator
- Workflows/automation
- Plugin API
