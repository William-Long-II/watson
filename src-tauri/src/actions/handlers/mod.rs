//! Per-variant handlers for `SearchAction`.
//!
//! `lib.rs::execute_action` originally fanned out a 100-line match
//! that mixed launch, open, paste, focus, and DB-side-effect logic
//! in a single function. Each match arm migrates here as its own
//! file with the side effects local to that arm — analogous to the
//! `search::providers` migration on the result side.
//!
//! Migration order (smallest first, validating the pattern before
//! tackling handlers with more state coupling):
//! 1. `launch_app` — DB write + in-memory cache patch + OS launch
//! 2. `open_url`, `open_file` — open via the OS, plus open-stats
//!    recording for files
//! 3. `copy_clipboard` — single delegation to ClipboardManager
//! 4. `run_command`, `paste_snippet` — delegate to actions::system
//!    and the OS-paste shim respectively
//! 5. `focus_window`, `focus_browser_tab` — cross-platform FFI
//!    surfaces, already have their own modules in `actions::*`
//! 6. `reindex_files` — interacts with FileIndexer + settings
//!
//! Frontend-only variants (`OpenNote`, `CreateNewNote`) stay no-op
//! Ok in the dispatcher and don't need a handler module.

pub mod copy_clipboard;
pub mod focus_browser_tab;
pub mod focus_window;
pub mod launch_app;
pub mod open_file;
pub mod open_url;
pub mod paste_snippet;
pub mod reindex_files;
pub mod run_command;
