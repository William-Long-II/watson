# Watson

A fast, cross-platform productivity launcher inspired by Alfred. Built with Tauri, React, and Rust.

![Watson Screenshot](docs/screenshot.png)

## Features

- **App Launcher** - Quickly find and launch applications with fuzzy search
- **Web Search** - Search Google, DuckDuckGo, GitHub, and more with keywords (e.g., `g query`)
- **Clipboard History** - Access your clipboard history with `cb` or `clip`
- **System Commands** - Run system commands with `>` prefix (sleep, restart, lock, etc.)
- **Custom Web Searches** - Add your own search engines with custom keywords
- **Script Commands** - Bind a keyword to your own script and show its JSON output as results
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
| `>left` / `>right` / `>max` / `>center` | Window management (Windows only in v1) |
| `;addr`, `;sig`, &hellip; | Snippets — type a trigger, paste the expansion into the focused app |
| `12 * 37` | Inline arithmetic — `sqrt(144)`, `(1+2)^3`, etc. |
| `30 C to F` | Unit conversion — temperature / length / weight |
| `100 USD to EUR` | Currency conversion (offline, snapshotted rates) |

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

#### Running tests

```bash
# Frontend unit tests (Vitest + React Testing Library)
npm test

# Rust tests (backend, storage, search)
cd src-tauri && cargo test
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

### Script Commands

A script command binds a keyword to a script on your machine. Typing the keyword, a space, and a query runs the script and shows what it prints as results.

1. Open Settings (gear) and scroll to "Script Commands"
2. Click "+ Add New" and enter:
   - **Name**: Display name (e.g., "Case")
   - **Keyword**: Trigger keyword, no spaces (e.g., `case`)
   - **Script path**: Path to the script (`~/` is expanded)
   - **Interpreter** (optional): Command used to run it. When blank, Watson picks one from the extension: `.py` → `python3` (`python` on Windows), `.js`/`.mjs`/`.cjs` → `node`, `.sh` → `sh`, `.rb` → `ruby`, `.ps1` → `powershell -File`. Anything else is executed directly, so a `chmod +x` script with a shebang or a `.exe`/`.bat` works too. On macOS, apps launched from the Dock don't see your shell's `PATH`, so give an absolute interpreter path (e.g. `/opt/homebrew/bin/node`) for tools installed by Homebrew.
   - **Icon** (optional): An emoji used for rows that don't set their own
3. Click Save, then type `case hello world`

`case ` with nothing after it runs the script with an empty query, which suits list-style scripts. Typing just `case` does not run it. A script keyword takes precedence over built-in prefixes, web searches and the calculator.

**How the script is run.** The query is passed as the first argument and in the `WATSON_QUERY` environment variable. The working directory is the script's folder and stdin is empty. The script is killed after 5 seconds (change `timeout_ms` on the entry in `config.toml`). A non-zero exit, a timeout, or invalid output shows a single "script error" row; selecting it copies the full message, including stderr. Scripts run with your own user permissions and are not sandboxed beyond what your OS provides, so only add scripts you trust.

**Output format (version 1).** Print one JSON object to stdout:

```json
{
  "version": 1,
  "items": [
    {
      "id": "london",
      "title": "London: 14°C, light rain",
      "subtitle": "Feels like 12°C · Enter to open forecast",
      "icon": "🌧",
      "preview": "Optional extra line shown under the subtitle",
      "action": { "type": "open_url", "url": "https://example.com/forecast/london" }
    }
  ]
}
```

| Field | Required | Meaning |
|-------|----------|---------|
| `version` | no | Contract version, defaults to `1`. Output with a higher version than Watson supports is rejected. |
| `items` | yes | Result rows, in the order they should appear. At most 50 are shown. |
| `items[].title` | yes | Main line. |
| `items[].subtitle` | no | Second line. Defaults to the command's name. |
| `items[].icon` | no | Emoji. Defaults to the command's icon. |
| `items[].preview` | no | Extra muted line under the subtitle. |
| `items[].id` | no | Stable id for the row. Defaults to its position. |
| `items[].action` | no | What Enter does. Defaults to copying `title`. |

Supported actions:

| `type` | Fields | Effect |
|--------|--------|--------|
| `open_url` | `url` | Opens an `http`, `https` or `mailto` URL in the default handler. |
| `copy_clipboard` | `content` | Copies `content` to the clipboard. |
| `open_file` | `path` | Opens a file or folder with its default app (`~/` is expanded). |

A bare JSON array of items is accepted as shorthand for version 1, and empty output means "no results". Unknown fields are ignored, so future versions can add fields without breaking existing scripts. A complete example lives in [`docs/script-commands/case.py`](docs/script-commands/case.py).

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
