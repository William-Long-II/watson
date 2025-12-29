# Watson

A fast, cross-platform productivity launcher inspired by Alfred. Built with Tauri, React, and Rust.

![Watson Screenshot](docs/screenshot.png)

## Features

- **App Launcher** - Quickly find and launch applications with fuzzy search
- **Web Search** - Search Google, DuckDuckGo, GitHub, and more with keywords (e.g., `g query`)
- **Clipboard History** - Access your clipboard history with `cb` or `clip`
- **System Commands** - Run system commands with `>` prefix (sleep, restart, lock, etc.)
- **Custom Web Searches** - Add your own search engines with custom keywords
- **Theming** - Light, dark, and system theme support
- **Global Hotkey** - Activate with Alt+Space (configurable)

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Alt+Space` | Show/hide Watson |
| `Enter` | Execute selected item |
| `Escape` | Hide Watson / Clear search |
| `Up/Down` | Navigate results |

## Quick Tips

| Command | Description |
|---------|-------------|
| `g <query>` | Search Google |
| `gh <query>` | Search GitHub |
| `yt <query>` | Search YouTube |
| `cb` | Show clipboard history |
| `cb <query>` | Search clipboard history |
| `> <command>` | Run system command |

## Installation

### Pre-built Binaries

Download the latest release for your platform from the [Releases](https://github.com/William-Long-II/watson/releases) page.

### Building from Source

#### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/) (latest stable)
- Platform-specific dependencies (see [Tauri Prerequisites](https://tauri.app/v1/guides/getting-started/prerequisites))

#### Build Steps

```bash
# Clone the repository
git clone git@github.com:William-Long-II/watson.git
cd watson

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Configuration

Watson stores its configuration in:
- **Linux**: `~/.config/watson/config.toml`
- **macOS**: `~/Library/Application Support/com.watson.app/config.toml`
- **Windows**: `%APPDATA%\watson\config.toml`

### Adding Custom Web Searches

1. Open Watson and click the settings icon (gear)
2. Scroll to "Web Searches"
3. Click "+ Add New"
4. Enter:
   - **Name**: Display name (e.g., "Jira")
   - **Keyword**: Trigger keyword (e.g., "jira")
   - **URL**: Search URL with `{query}` placeholder (e.g., `https://mycompany.atlassian.net/browse/{query}`)
5. Click Save

## Tech Stack

- **Frontend**: React 18, TypeScript, Tailwind CSS v4, Zustand
- **Backend**: Rust, Tauri 2.x
- **Database**: SQLite (via rusqlite)
- **Search**: Fuzzy matching with skim

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- Inspired by [Alfred](https://www.alfredapp.com/) for macOS
- Built with [Tauri](https://tauri.app/)
- Icon: Watson's iconic bowler hat
