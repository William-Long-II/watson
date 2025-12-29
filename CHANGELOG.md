# Changelog

All notable changes to Watson will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
