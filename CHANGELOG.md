# Changelog

All notable changes to Watson will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
