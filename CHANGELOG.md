# Changelog

All notable changes to Watson will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]


## [1.6.1] - 2026-05-02

### Fixed
- **Snippet paste reliability** - Replaces the previous PowerShell SendKeys / osascript shell-outs with direct event injection (Win32 `SendInput` on Windows, Quartz `CGEventPost` on macOS). Fixes a Windows-only NumLock-toggle bug where typing a snippet trigger and hitting Enter could close Watson and toggle NumLock instead of pasting (KB179987-style foreground-change race). On macOS, eliminates the second permission prompt for "System Events" Automation — the existing Accessibility grant now covers snippets too.


## [1.6.0] - 2026-05-02

### Added
- **Cross-Platform Switcher Parity** - The "Switch to" feature now works on all three desktop platforms. Search any open window's title or browser tab to focus it directly:
  - **macOS** (#61, #65) - Window enumeration + focus via the Accessibility API (`AXUIElement`); browser tabs via AX tree walk for Safari, Chromium-family (Chrome / Brave / Edge / Vivaldi / Opera) and Arc. Cross-Space activation via `NSRunningApplication.activate`.
  - **Linux X11** (#62) - Window enumeration + focus via EWMH (`_NET_CLIENT_LIST` + `_NET_ACTIVE_WINDOW`) using pure-Rust `x11rb`. Browser tabs (#66) via AT-SPI for Firefox, Chromium-family.
  - **Linux Wayland** - Window switcher on wlroots-based compositors (Sway, Hyprland, river, Wayfire) via `wlr-foreign-toplevel-management-unstable-v1`. GNOME/Mutter and KDE/KWin surface a clear "compositor unsupported" message.
- **macOS Accessibility Onboarding** - First-launch banner detects when the Accessibility permission isn't granted, with one-click "Grant access" (deep-link to System Settings → Privacy & Security → Accessibility) and "Re-check" (re-probes without restarting Watson).

### Changed
- **`focus_window` reliability on Windows** - Direct Win32 FFI replaces the prior PowerShell shell-out so multi-window apps (Brave, VS Code, Slack workspaces) correctly enumerate one row per window instead of one per process. Foreground rights handled via `AttachThreadInput`.

### Technical
- New per-platform deps under `cfg(target_os)` gates: `x11rb`, `zbus` (with `blocking`), `wayland-client`, `wayland-protocols-wlr` on Linux; `core-foundation`, `objc2-app-kit` on macOS; `windows = "0.61"` UIA features on Windows.
- AT-SPI client uses raw zbus proxies (no `atspi` crate) so the dep surface stays small and the search hot path remains synchronous.
- Wayland backend uses per-call connections + 4 roundtrips to drain the protocol bootstrap; toplevel identity is a SipHash of `(app_id, title)` masked to JSON's safe-integer range.


## [1.5.1] - 2026-05-01

### Added
- **Browser Tab Switching** - Browser tabs (Brave, Chrome, Edge, Firefox, Vivaldi, Opera, Arc) now appear individually in search results. Selecting one calls UIA `SelectionItemPattern.Select()` on the underlying TabItem and brings the parent window forward.
- **Multi-Window Switcher** - Each open window of an app surfaces as its own row instead of one-per-process. Switcher uses Win32 `EnumWindows` with a filter pipeline (visible / non-toolwindow / titled / non-cloaked).
- **Empty-State Affordances** - The notes route always surfaces a "Create new note" entry, and the files route always surfaces a "Re-index files now" entry, so users with empty databases have a discoverable next step.

### Fixed
- **Switch-to-Window on Windows** - `focus_window` now uses direct Win32 FFI (`SetForegroundWindow` + `AttachThreadInput`) instead of a PowerShell shell-out. The previous path could not satisfy Windows' foreground-rights model and silently failed.
- **Bare-Letter Shortcuts Eating Searches** - Typing a query starting with `n`, `N`, `f`, or `c` no longer triggers the corresponding internal shortcut. The shortcuts now require a trailing space (`n `, `f `, `cb `) so searches like "Slack" or "node" pass through to results. Bare backtick still opens the scratchpad.


## [1.5.0] - 2026-05-01

## [1.4.0] - 2026-05-01

### Added
- **Gemini Improvements**:
  - **Background File I/O** - Note saving and loading now happen on a background thread for zero UI lag.
  - **Frecency Ranking** - Search results for apps and files are intelligently ranked by both usage frequency and recency.
  - **'Switch to' Windows** - Open windows and browser tabs now appear in search results, allowing instant focus instead of re-launching.
  - **Note Previews** - Inline content snippets in search results give a quick peek into your note contents.
  - **Transactional Integrity** - Guaranteed synchronization between the database and markdown files for notes.
  - **Auto Icon Refresh** - Icons are automatically re-extracted and updated whenever an application is patched by the OS.
- **Snippets** - User-defined text expansion with `;` trigger support and cross-platform paste (WAT-301).
- **Secondary Actions** - Per-result Cmd+K menu for advanced actions like "Reveal in folder" and "Copy path" (WAT-404).
- **Notifications Drawer** - A persistent history of non-fatal events and startup warnings (WAT-406).
- **Window Management** - New system commands for window tiling (`split-left`, `maximize`, `center`, etc.) (WAT-302).
- **Clipboard Pinning** - Persist important clipboard entries across restarts (WAT-303).
- **Clipboard Privacy** - Configurable regex patterns to automatically exclude sensitive data from clipboard history (WAT-303).
- **Confirmation Modals** - Safeguards for destructive actions like deleting notes or clearing history (WAT-405).

### Changed
- **Responsive Layout** - Window width now scales based on monitor resolution; window re-centers on activation (WAT-407).
- **A11y Enhancements** - Comprehensive accessibility audit: improved ARIA roles, combobox semantics, and keyboard navigation (WAT-402).

### Technical
- Migrated to asynchronous note storage using `tokio`.
- Implemented SQLite migration framework v006.
- Added platform shims for window enumeration and focusing.

### Added
- **Notes** - Create, edit, and search markdown notes with `n` prefix (e.g., `n meeting`)
- **Scratchpad** - Quick text capture area accessible via `s` or backtick key
- **File Search** - Search indexed files with `f` prefix (e.g., `f config`)
- **Chained Shortcuts** - Single-key shortcuts when search is empty:
  - `n` - Create new note
  - `N` (Shift+n) - Search notes
  - `f` - Search files
  - `s` or backtick - Open scratchpad
- File search settings panel with configurable indexed paths and exclusion patterns
- Note editor with auto-save, delete confirmation, and keyboard shortcuts (Cmd/Ctrl+S to save)

### Fixed
- Enabled createUpdaterArtifacts in bundle config for signed updates


## [1.2.6] - 2025-12-30

## [1.2.5] - 2025-12-30

### Fixed
- Fixed truncated public key in updater config
- Pinned tauri-action to v0.5.16 to fix latest.json generation bug


## [1.2.4] - 2025-12-30

## [1.2.3] - 2025-12-30

### Fixed
- Auto-updater now generates `latest.json` manifest (added tagName to tauri-action)


## [1.2.2] - 2025-12-30

### Changed
- Updated macOS x86_64 runner from deprecated macos-13 to macos-15-intel

## [1.2.1] - 2025-12-30

### Added
- Tab/Shift+Tab keyboard shortcuts to cycle through search results


## [1.2.0] - 2025-12-30

### Fixed
- Release workflow multiline output handling (EOF delimiter issue)
- Release workflow bash/node script syntax errors

### Security
- Regenerated signing keys after accidental commit
- Added `*.key` patterns to .gitignore

## [1.1.1] - 2024-12-29

### Added
- Auto-update functionality with "Check for Updates" button in settings
- Automatic download and install of updates from GitHub releases

### Fixed
- Window dragging now works on Windows (added missing permissions)
- Windows transparency issues resolved (removed blue border and shadow artifacts)
- Version display now shows actual version from app config instead of hardcoded value

### Changed
- Disabled window transparency for better cross-platform compatibility
- Window now uses solid background color


## [1.0.2] - 2024-12-29

### Fixed
- macOS DMG bundling in GitHub Actions (switched to tauri-action)

### Changed
- Combined version bump and release into single workflow
- Release workflow now triggered manually with version bump selection

## [1.0.1] - 2024-12-29

### Fixed
- GitHub Actions CI workflow (corrected rust-toolchain action name)
- Windows icon now proper ICO format with multiple sizes (16-256px)
- Added icon.png and icon.ico to Tauri bundle configuration

## [1.0.0] - 2024-12-29

### Added
- App launcher with fuzzy search
- Web search with customizable keywords (Google, DuckDuckGo, GitHub, YouTube, etc.)
- Clipboard history with `cb` or `clip` command
- System commands with `>` prefix (sleep, restart, lock, etc.)
- Custom web searches with instance support (e.g., Jira, Confluence)
- Theme support (light, dark, system)
- Global hotkey activation (Alt+Space)
- Dynamic window resizing
- Settings panel with quick configuration
- Watson bowler hat icon

### Technical
- Built with Tauri 2.x, React 18, TypeScript, Tailwind CSS v4
- SQLite database for persistent storage
- Fuzzy matching with skim algorithm
- Cross-platform support (Linux, macOS, Windows)
