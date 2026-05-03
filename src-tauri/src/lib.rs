mod actions;
mod apps;
mod calculator;
mod clipboard;
mod config;
mod db;
mod files;
mod indexers;
mod layout;
mod notes;
mod notifications;
mod scratchpad;
mod search;
mod snippets;
mod warnings;

use actions::system::{execute_command, get_system_commands};
use clipboard::ClipboardManager;
use config::settings::Settings;
use files::{FileEntry, FileSearchManager, indexer::FileIndexer};
use notes::NotesManager;
use notifications::NotificationsManager;
use scratchpad::ScratchpadManager;
use snippets::SnippetsManager;
use warnings::{StartupWarning, StartupWarnings};
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
use search::dispatch::{classify_prefix_route, Route, SubQuery};
use search::provider::ResultProvider;
use search::providers::apps::AppsProvider;
use search::providers::browser_tabs::BrowserTabsProvider;
use search::providers::calculator::CalculatorProvider;
use search::providers::snippets::SnippetsProvider;
use search::providers::system_commands::SystemCommandsProvider;
use search::providers::web_search::WebSearchProvider;
use search::providers::windows::WindowsProvider;
use search::{ResultType, SearchAction, SearchEngine, SearchResult};
use std::sync::{Arc, RwLock};
use tauri::{Manager, State};

#[cfg(not(target_os = "linux"))]
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
    clipboard: ClipboardManager,
    scratchpad: ScratchpadManager,
    notes: NotesManager,
    file_search: Arc<FileSearchManager>,
    /// WAT-301: user-defined text snippets. Surface in search; on
    /// execute, copy expansion and paste into the prior-focused window.
    snippets: SnippetsManager,
    /// WAT-105: non-fatal conditions that surfaced during `setup()` and
    /// should be rendered in the UI — e.g., the global hotkey couldn't be
    /// registered because another app already owns it.
    startup_warnings: StartupWarnings,
    /// WAT-406: in-memory drawer of non-fatal events. Startup warnings
    /// also push here so the drawer keeps a visible history after the
    /// banner is dismissed.
    notifications: NotificationsManager,
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

#[tauri::command]
fn save_settings_cmd(settings: Settings, state: State<AppState>) -> Result<(), String> {
    config::save_settings(&settings)?;
    // WAT-303: hot-reload clipboard ignore patterns so changes take
    // effect on the next monitor tick without an app restart. Any
    // compile errors are logged to stderr (silent on the happy path);
    // the user sees them at the next startup via the banner.
    let (patterns, errors) = clipboard::compile_ignore_patterns(&settings.clipboard.ignore_patterns);
    if !errors.is_empty() {
        eprintln!("watson: clipboard filter errors on save: {}", errors.join("; "));
    }
    state.clipboard.set_ignore_patterns(patterns);
    *state.settings.write().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn reindex_apps(state: State<AppState>) -> usize {
    let indexer = get_indexer();
    let mut apps = indexer.index_apps();
    // WAT-201: the indexer resets launch_count/last_launched to their
    // indexer defaults. Re-merge persisted stats so ranking survives a
    // reindex. Failure is non-fatal — without stats, ranking falls back
    // to fuzzy-score-only behavior.
    let _ = apps::merge_stats_into_apps(&state.db, &mut apps);
    let count = apps.len();
    *state.indexed_apps.write().unwrap() = apps;
    count
}

/// Gemini Improvement #4: clean plain-text snippet of a note's content.
/// Strips markdown headers and collapses newlines to provide a dense
/// preview for the search results row.
fn note_preview(content: &str) -> String {
    let clean = content
        .lines()
        .filter(|line| !line.trim().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    let clipped: String = clean.chars().take(100).collect();
    if clean.chars().count() > 100 {
        format!("{clipped}…")
    } else {
        clipped
    }
}

async fn notes_route_results(state: &State<'_, AppState>, sub: SubQuery) -> Vec<SearchResult> {
    let notes_res = match sub {
        SubQuery::Listing => state.notes.get_recent(8).await,
        SubQuery::Search(ref q) if q.is_empty() => state.notes.get_recent(8).await,
        SubQuery::Search(q) => state.notes.search(&q).await,
    };
    let mut out: Vec<SearchResult> = notes_res
        .map(|notes| {
            notes
                .into_iter()
                .take(8)
                .map(|note| SearchResult {
                    id: note.id.clone(),
                    name: note.title,
                    description: format!("Note · {}", note.tags.join(", ")),
                    icon: Some("note".to_string()),
                    result_type: ResultType::Note,
                    score: 10000,
                    frecency_score: 0.0,
                    preview: Some(note_preview(&note.content)),
                    pinned: false,
                    action: SearchAction::OpenNote { note_id: note.id },
                })
                .collect()
        })
        .unwrap_or_default();

    // Always surface a "Create new note" entry in the notes route.
    // This is the replacement for the bare-`n` keyboard shortcut that
    // used to open the editor — without it, a user with zero notes
    // who types `n` sees an empty list and can't tell that the route
    // even fired.
    out.push(SearchResult {
        id: "note:__create__".to_string(),
        name: "Create new note".to_string(),
        description: "Open the editor with an empty note".to_string(),
        icon: Some("note_new".to_string()),
        result_type: ResultType::Note,
        // Lower than recent notes (10000) so it sorts to the bottom
        // when notes exist, but is the only result when they don't.
        score: 1,
        frecency_score: 0.0,
        preview: None,
        pinned: false,
        action: SearchAction::CreateNewNote,
    });
    out
}

async fn files_route_results(state: &State<'_, AppState>, sub: SubQuery) -> Vec<SearchResult> {
    let files = match sub {
        SubQuery::Listing => state.file_search.get_recent(8),
        SubQuery::Search(ref q) if q.is_empty() => state.file_search.get_recent(8),
        SubQuery::Search(q) => state.file_search.search(&q, 8),
    };
    let mut out: Vec<SearchResult> = files
        .map(|files| {
            files
                .into_iter()
                .map(|file| SearchResult {
                    id: file.id.clone(),
                    name: file.name,
                    description: file.path.clone(),
                    icon: Some("file".to_string()),
                    result_type: ResultType::File,
                    score: 10000,
                    frecency_score: 0.0,
                    preview: None,
                    pinned: false,
                    action: SearchAction::OpenFile { path: file.path },
                })
                .collect()
        })
        .unwrap_or_default();

    // Always surface a "Re-index files now" affordance — same pattern
    // as the notes route's "Create new note". A user with an empty
    // file index types `f` and otherwise sees nothing; this entry
    // makes the empty-state visible and gives them a one-click path
    // to populate the index from whatever paths are configured in
    // Settings.
    out.push(SearchResult {
        id: "file:__reindex__".to_string(),
        name: "Re-index files now".to_string(),
        description: "Re-scan the configured indexed paths".to_string(),
        icon: Some("file_reindex".to_string()),
        result_type: ResultType::File,
        score: 1, // sorts to the bottom when real files exist
        frecency_score: 0.0,
        preview: None,
        pinned: false,
        action: SearchAction::ReindexFiles,
    });
    out
}

// Phase 1A: `calculation_result` and `system_commands_route_results`
// have moved to `search::providers::calculator` and
// `search::providers::system_commands` respectively. The dispatcher
// in `search()` instantiates each provider per call.

/// WAT-304: build the empty-query default view — up to 5 recently-launched
/// apps + up to 5 recently-opened files, interleaved by timestamp so the
/// most recently touched item comes first regardless of kind. Returns an
/// empty vec on a fresh install (no launches or opens yet); the UI then
/// falls back to its quick-tips placeholder.
fn recents_results(state: &State<'_, AppState>) -> Vec<SearchResult> {
    const PER_KIND: usize = 5;
    const TOTAL_LIMIT: usize = 10;

    // Collect (timestamp, result) pairs so the final sort is a single
    // pass over both kinds.
    let mut combined: Vec<(i64, SearchResult)> = Vec::with_capacity(PER_KIND * 2);

    // Recent apps — filter to those actually launched (count > 0 AND
    // timestamp present). Sort by last_launched desc and take top N.
    {
        let apps = state.indexed_apps.read().unwrap();
        let mut launched: Vec<&AppEntry> = apps
            .iter()
            .filter(|a| a.launch_count > 0 && a.last_launched.is_some())
            .collect();
        launched.sort_by(|a, b| b.last_launched.cmp(&a.last_launched));
        for app in launched.into_iter().take(PER_KIND) {
            let ts = app.last_launched.unwrap_or(0);
            combined.push((
                ts,
                SearchResult {
                    id: app.id.clone(),
                    name: app.name.clone(),
                    description: "Recently launched".to_string(),
                    icon: app.icon_cache_path.clone(),
                    result_type: ResultType::Application,
                    score: 10000,
                    frecency_score: 0.0,
                    preview: None,
                    pinned: false,
                    action: SearchAction::LaunchApp { path: app.path.clone() },
                },
            ));
        }
    }

    // Recent files — already sorted in SQL.
    if let Ok(files) = state.file_search.get_recent_opened(PER_KIND) {
        for (file, ts) in files {
            combined.push((
                ts,
                SearchResult {
                    id: file.id,
                    name: file.name,
                    description: format!("Recently opened · {}", file.path),
                    icon: Some("file".to_string()),
                    result_type: ResultType::File,
                    score: 10000,
                    frecency_score: 0.0,
                    preview: None,
                    pinned: false,
                    action: SearchAction::OpenFile { path: file.path },
                },
            ));
        }
    }

    combined.sort_by(|a, b| b.0.cmp(&a.0));
    combined
        .into_iter()
        .take(TOTAL_LIMIT)
        .map(|(_, r)| r)
        .collect()
}

fn clipboard_route_results(state: &State<'_, AppState>, sub: SubQuery) -> Vec<SearchResult> {
    let entries = match sub {
        SubQuery::Listing => state.clipboard.get_history(),
        SubQuery::Search(ref q) if q.is_empty() => state.clipboard.get_history(),
        SubQuery::Search(q) => state.clipboard.search_history(&q),
    };
    entries
        .into_iter()
        .take(8)
        .map(|entry| SearchResult {
            id: entry.id,
            name: entry.preview.clone(),
            description: if entry.pinned {
                format!("📌 Pinned · Copied {}", entry.timestamp.format("%H:%M:%S"))
            } else {
                format!("Copied {}", entry.timestamp.format("%H:%M:%S"))
            },
            icon: Some("clipboard".to_string()),
            result_type: ResultType::Clipboard,
            score: 10000,
            frecency_score: 0.0,
            preview: None,
            pinned: entry.pinned,
            action: SearchAction::CopyClipboard { content: entry.content },
        })
        .collect()
}

#[tauri::command]
async fn search(query: String, state: State<'_, AppState>) -> Result<Vec<SearchResult>, String> {
    // WAT-203: inline calculator runs first and short-circuits when the
    // query looks like math or a unit/currency conversion. The detector
    // is conservative — ordinary search queries ("apple", "chrome",
    // "iphone 15") fall through to the normal pipeline.
    // WAT-203 short-circuit: if the query parses as math / unit /
    // currency, skip the rest of the pipeline and return only the
    // calculator's row. The provider returns an empty Vec for
    // non-math input, so checking for non-empty is the discriminator.
    let calc_results = CalculatorProvider.search(&query);
    if !calc_results.is_empty() {
        return Ok(calc_results);
    }

    match classify_prefix_route(&query) {
        Route::Empty => return Ok(recents_results(&state)),
        Route::Notes(sub) => return Ok(notes_route_results(&state, sub).await),
        Route::Files(sub) => return Ok(files_route_results(&state, sub).await),
        Route::Clipboard(sub) => return Ok(clipboard_route_results(&state, sub)),
        Route::SystemCommands(sub) => return Ok(SystemCommandsProvider { sub }.search(&query)),
        Route::Passthrough => {}
    }

    let settings = state.settings.read().unwrap();
    let apps = state.indexed_apps.read().unwrap();

    let mut items: Vec<SearchResult> = Vec::new();

    // Phase 1A: web-search keyword matching now flows through a
    // `ResultProvider` impl. Same external behavior — classification +
    // URL construction reuse the existing helpers — but the
    // dispatcher no longer hand-rolls the SearchResult struct.
    items.extend(
        WebSearchProvider {
            configs: &settings.web_searches,
        }
        .search(&query),
    );

    // Phase 1A: snippets via `ResultProvider`. WAT-301 — runs as a
    // regular search-pipeline contributor alongside apps and web,
    // no special prefix. Users who want scoped lookup can use a
    // `;`-prefixed trigger by convention.
    items.extend(
        SnippetsProvider {
            manager: &state.snippets,
        }
        .search(&query),
    );

    // Phase 1A: apps via `ResultProvider`. The dispatcher gates the
    // call on `!query.contains(' ') || items.is_empty()` (single-word
    // query, OR fall back when nothing else matched) — that perf trim
    // stays here because it short-circuits the entire app emission,
    // not just per-app filtering.
    if !query.contains(' ') || items.is_empty() {
        items.extend(
            AppsProvider {
                apps: &apps,
                use_frequency_ranking: settings.search.use_frequency_ranking,
                now: chrono::Utc::now().timestamp(),
            }
            .search(&query),
        );
    }

    // Phase 1A: open windows + browser tabs via two `ResultProvider`
    // impls that share a single `get_open_windows()` enumeration —
    // the cross-platform FFI work runs once per query, not twice.
    // Errors are silenced here (same policy as before): an empty
    // window list yields zero results from both providers.
    let open_windows = actions::windows::get_open_windows().unwrap_or_default();
    items.extend(
        BrowserTabsProvider {
            windows: &open_windows,
        }
        .search(&query),
    );
    items.extend(
        WindowsProvider {
            windows: &open_windows,
        }
        .search(&query),
    );

    // Gemini Improvement #2: Add files with frecency bonus.
    if let Ok(files) = state.file_search.get_recent(50) {
        for file in files {
            items.push(SearchResult {
                id: file.id.clone(),
                name: file.name,
                description: file.path.clone(),
                icon: Some("file".to_string()),
                result_type: ResultType::File,
                score: 0,
                frecency_score: 0.0,
                preview: None,
                pinned: false,
                action: SearchAction::OpenFile { path: file.path },
            });
        }
    }

    // Search and limit results
    let mut results = state.search_engine.search(&query, items);
    results.truncate(settings.search.max_results);
    Ok(results)
}

#[tauri::command]
fn execute_action(action: SearchAction, state: State<AppState>) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => {
            // WAT-201: record the launch BEFORE invoking the OS; if the
            // launch itself fails, the stats still reflect intent (the
            // user chose this app). Also patch the in-memory cache so
            // the next search reflects the updated count without waiting
            // for a reindex round-trip.
            let _ = apps::record_launch(&state.db, &path);
            let now = chrono::Utc::now().timestamp();
            {
                let mut cached = state.indexed_apps.write().unwrap();
                for app in cached.iter_mut() {
                    if app.path == path {
                        app.launch_count = app.launch_count.saturating_add(1);
                        app.last_launched = Some(now);
                        break;
                    }
                }
            }
            actions::launch_app(&path)
        }
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
        SearchAction::CopyClipboard { content } => state.clipboard.copy_to_clipboard(&content),
        SearchAction::OpenNote { note_id: _ } => {
            // Note opening is handled by the frontend
            Ok(())
        }
        SearchAction::OpenFile { path } => {
            // WAT-304: record open before invoking the OS so stats
            // reflect user intent even if the OS-level open fails.
            // Failure to record is non-fatal — the open proceeds.
            let _ = state.file_search.record_open(&path);
            open::that(&path).map_err(|e| e.to_string())
        }
        SearchAction::PasteSnippet { expansion } => {
            // WAT-301: two steps. (1) Put the expansion on the
            // clipboard — this alone is useful even if step 2 fails.
            // (2) Synthesize Ctrl/Cmd+V into the prior-focused window.
            // The frontend hides Watson before calling us, so focus
            // has returned to wherever the user was typing.
            state.clipboard.copy_to_clipboard(&expansion)?;
            paste_snippet_via_os()
        }
        SearchAction::FocusWindow { hwnd } => actions::windows::focus_window(hwnd),
        SearchAction::FocusBrowserTab { hwnd, index } => {
            actions::browser_tabs::focus_browser_tab(hwnd, index)
        }
        // Frontend handles `CreateNewNote` directly (opens the editor
        // panel). If it ever reaches the backend, that's a frontend
        // routing bug — return Ok so the user doesn't see an error
        // toast for what's effectively a no-op.
        SearchAction::CreateNewNote => Ok(()),
        // Trigger file re-index. Frontend will refresh the search
        // results once this returns so the newly-indexed files
        // surface immediately.
        SearchAction::ReindexFiles => {
            let settings = state.settings.read().unwrap();
            if !settings.file_search.enabled {
                return Err("File search is disabled in Settings".to_string());
            }
            state.file_search.reset_cancel();
            let indexer = files::indexer::FileIndexer::new(
                Arc::clone(&state.file_search),
                settings.file_search.indexed_paths.clone(),
                settings.file_search.excluded_patterns.clone(),
                settings.file_search.max_depth,
            );
            indexer.index_all();
            Ok(())
        }
    }
}

/// WAT-301: platform shim for synthesizing a paste keystroke.
/// Windows uses Win32 `SendInput` directly. macOS uses Quartz
/// `CGEventPost` directly. Linux is best-effort via xdotool;
/// if xdotool isn't installed, the expansion is still on the
/// clipboard and the user can paste manually.
///
/// Why direct event APIs and not OS-level scripting (PowerShell
/// SendKeys on Windows, osascript System Events on macOS):
///
/// - **Windows / SendKeys**: raced the foreground-window change
///   Watson triggers when hiding, and intermittently toggled
///   NumLock instead of pasting (KB179987-style behavior).
/// - **macOS / osascript**: requires Automation permission, a
///   second permission prompt on top of the Accessibility
///   permission we already require for the switcher. CGEvent
///   uses the Accessibility grant, so first-launch UX is
///   one prompt instead of two.
///
/// Both new paths also drop the cold-start cost of spawning a
/// scripting host (~100-150ms on every paste).
#[cfg(target_os = "windows")]
fn paste_snippet_via_os() -> Result<(), String> {
    use std::thread::sleep;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_V,
    };

    // Give the previously-focused window a moment to fully regain
    // focus after Watson hides. Without this, the synthesized
    // Ctrl+V can race the activation and land on the desktop or on
    // Watson itself (now hidden).
    sleep(Duration::from_millis(80));

    fn key(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    // Down-down-up-up so Ctrl is held while V is struck.
    let inputs = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];

    let n = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if n != inputs.len() as u32 {
        return Err(format!(
            "SendInput accepted only {n} of {} keystrokes",
            inputs.len()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn paste_snippet_via_os() -> Result<(), String> {
    use std::os::raw::c_void;
    use std::thread::sleep;
    use std::time::Duration;

    type CGEventRef = *mut c_void;
    type CGEventSourceRef = *mut c_void;
    type CGEventTapLocation = u32;
    type CGEventFlags = u64;
    type CGKeyCode = u16;
    type CFTypeRef = *const c_void;

    /// Quartz session-level tap: posts after WindowServer processes
    /// events but before they reach apps in the user's session. The
    /// right level for "I'm a userland tool synthesizing input."
    const K_CG_SESSION_EVENT_TAP: CGEventTapLocation = 1;
    /// Modifier flag mask for Command. Apple's `CGEventFlags`
    /// bits — `kCGEventFlagMaskCommand`.
    const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x100000;
    /// HIToolbox `kVK_ANSI_V` — the V key's layout-independent
    /// virtual position. Cmd+V is the universal paste shortcut so
    /// the physical position is what matters, not the user's
    /// keyboard layout.
    const K_VK_ANSI_V: CGKeyCode = 0x09;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
        fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
    }

    // Same 80ms grace period as the Windows path: the previously-
    // focused window needs a beat to fully regain focus after
    // Watson hides, otherwise the synthesized keystrokes can land
    // on Watson itself or on the desktop.
    sleep(Duration::from_millis(80));

    unsafe fn post_v_event(key_down: bool) -> Result<(), String> {
        let event = CGEventCreateKeyboardEvent(std::ptr::null_mut(), K_VK_ANSI_V, key_down);
        if event.is_null() {
            return Err(String::from(
                "CGEventCreateKeyboardEvent returned null — Accessibility \
                 permission probably not granted",
            ));
        }
        CGEventSetFlags(event, K_CG_EVENT_FLAG_MASK_COMMAND);
        CGEventPost(K_CG_SESSION_EVENT_TAP, event);
        CFRelease(event as CFTypeRef);
        Ok(())
    }

    unsafe {
        post_v_event(true)?; // Cmd+V down
        post_v_event(false)?; // Cmd+V up
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn paste_snippet_via_os() -> Result<(), String> {
    // Linux best-effort. `xdotool` is widely available but not
    // guaranteed; if it's missing, the snippet is already on the
    // clipboard so the user can paste manually (Ctrl+V).
    match std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "Could not synthesize paste (xdotool not found: {e}). The snippet is on your clipboard — press Ctrl+V manually."
        )),
    }
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    window.hide().ok();
}

#[tauri::command]
fn show_window(window: tauri::WebviewWindow) {
    window.show().ok();
    window.set_focus().ok();
    // WAT-407: re-center on show so the window lands on the middle
    // of whichever monitor it will appear on (useful after a
    // disconnect/reconnect that leaves it offscreen).
    center_window_on_current_monitor(&window);
}

#[tauri::command]
fn resize_window(window: tauri::WebviewWindow, height: u32) {
    use tauri::LogicalSize;
    // WAT-407: scale width to monitor so Watson doesn't look tiny on
    // ultrawide / 4K displays. Falls back to 600 (the pre-WAT-407
    // default) when monitor info isn't available.
    let width = window_width_for_current_monitor(&window);
    let _ = window.set_size(LogicalSize::new(width, height));
}

/// WAT-407: look up the monitor the window is currently on, convert
/// its physical size to logical pixels, and pick the Watson width
/// from the breakpoints table. Defaults to 600 if the OS can't
/// report a monitor (rare — unplugged display mid-hotkey).
fn window_width_for_current_monitor(window: &tauri::WebviewWindow) -> u32 {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return layout::DEFAULT_WIDTH;
    };
    let scale = monitor.scale_factor();
    // Avoid divide-by-zero on pathological scale factors.
    if scale <= 0.0 {
        return layout::DEFAULT_WIDTH;
    }
    let logical_width = monitor.size().width as f64 / scale;
    layout::compute_window_width(logical_width)
}

/// WAT-407: center horizontally on the current monitor, preserving
/// the window's current Y. Called from `show_window` so a re-showing
/// window always lands within a sensible display rather than
/// lingering offscreen after a monitor change.
fn center_window_on_current_monitor(window: &tauri::WebviewWindow) {
    use tauri::LogicalPosition;
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        return;
    }
    let mon_size = monitor.size();
    let mon_pos = monitor.position();
    let mon_logical_width = mon_size.width as f64 / scale;
    let mon_logical_x = mon_pos.x as f64 / scale;
    let width = window_width_for_current_monitor(window);
    let center_x = layout::center_x_on_monitor(mon_logical_x, mon_logical_width, width);
    // Preserve Y so we don't fight the user's vertical choice. Falls
    // back to the monitor's y origin if outer_position fails.
    let current_y = match window.outer_position() {
        Ok(p) => p.y as f64 / scale,
        Err(_) => mon_pos.y as f64 / scale,
    };
    let _ = window.set_position(LogicalPosition::new(center_x, current_y));
}

#[tauri::command]
fn get_clipboard_history(state: State<AppState>) -> Vec<clipboard::ClipboardEntry> {
    state.clipboard.get_history()
}

#[tauri::command]
fn search_clipboard(query: String, state: State<AppState>) -> Vec<clipboard::ClipboardEntry> {
    state.clipboard.search_history(&query)
}

#[tauri::command]
fn clear_clipboard_history(state: State<AppState>) {
    state.clipboard.clear_history();
}

#[tauri::command]
fn copy_to_clipboard(content: String, state: State<AppState>) -> Result<(), String> {
    state.clipboard.copy_to_clipboard(&content)
}

/// WAT-303: mark a clipboard entry as pinned. Persists to DB; the pin
/// survives `clear_clipboard_history` and app restarts.
#[tauri::command]
fn pin_clipboard_entry(id: String, state: State<AppState>) -> Result<bool, String> {
    state.clipboard.pin(&id)
}

/// WAT-303: unpin a clipboard entry.
#[tauri::command]
fn unpin_clipboard_entry(id: String, state: State<AppState>) -> Result<bool, String> {
    state.clipboard.unpin(&id)
}

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

#[tauri::command]
async fn create_note(title: String, content: String, state: State<'_, AppState>) -> Result<notes::Note, String> {
    state.notes.create(&title, &content).await
}

#[tauri::command]
async fn update_note(id: String, title: String, content: String, state: State<'_, AppState>) -> Result<notes::Note, String> {
    state.notes.update(&id, &title, &content).await
}

#[tauri::command]
async fn delete_note(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.notes.delete(&id).await
}

#[tauri::command]
async fn get_note(id: String, state: State<'_, AppState>) -> Result<Option<notes::Note>, String> {
    state.notes.get(&id).await
}

/// WAT-204: adopt the on-disk version of a note, overwriting the DB. The
/// frontend reconcile dialog calls this when the user picks "Use disk".
#[tauri::command]
async fn reload_note_from_disk(id: String, state: State<'_, AppState>) -> Result<notes::Note, String> {
    state.notes.reload_from_disk(&id).await
}

#[tauri::command]
async fn search_notes(query: String, state: State<'_, AppState>) -> Result<Vec<notes::Note>, String> {
    state.notes.search(&query).await
}

#[tauri::command]
async fn get_recent_notes(limit: usize, state: State<'_, AppState>) -> Result<Vec<notes::Note>, String> {
    state.notes.get_recent(limit).await
}

#[tauri::command]
fn search_files(query: String, limit: usize, state: State<AppState>) -> Result<Vec<FileEntry>, String> {
    state.file_search.search(&query, limit)
}

#[tauri::command]
fn search_files_by_extension(extension: String, limit: usize, state: State<AppState>) -> Result<Vec<FileEntry>, String> {
    state.file_search.search_by_extension(&extension, limit)
}

#[tauri::command]
fn get_recent_files(limit: usize, state: State<AppState>) -> Result<Vec<FileEntry>, String> {
    state.file_search.get_recent(limit)
}

#[tauri::command]
fn reindex_files(state: State<AppState>) -> usize {
    let settings = state.settings.read().unwrap();
    if !settings.file_search.enabled {
        return 0;
    }

    // R-07: clear any cancel requested against a prior run before starting.
    state.file_search.reset_cancel();

    let indexer = FileIndexer::new(
        Arc::clone(&state.file_search),
        settings.file_search.indexed_paths.clone(),
        settings.file_search.excluded_patterns.clone(),
        settings.file_search.max_depth,
    );
    indexer.index_all()
}

/// Ask the currently-running file indexer to stop. Returns immediately;
/// the indexer checks the flag between entries so the actual stop happens
/// at the next boundary.
#[tauri::command]
fn cancel_reindex_files(state: State<AppState>) {
    state.file_search.request_cancel();
}

#[tauri::command]
fn clear_file_index(state: State<AppState>) -> Result<(), String> {
    state.file_search.clear_all()
}

// --- WAT-301: snippet CRUD ---

#[tauri::command]
fn list_snippets(state: State<AppState>) -> Result<Vec<snippets::Snippet>, String> {
    state.snippets.list()
}

#[tauri::command]
fn create_snippet(
    trigger: String,
    name: String,
    expansion: String,
    state: State<AppState>,
) -> Result<snippets::Snippet, String> {
    state.snippets.create(&trigger, &name, &expansion)
}

#[tauri::command]
fn update_snippet(
    id: String,
    trigger: String,
    name: String,
    expansion: String,
    state: State<AppState>,
) -> Result<snippets::Snippet, String> {
    state.snippets.update(&id, &trigger, &name, &expansion)
}

#[tauri::command]
fn delete_snippet(id: String, state: State<AppState>) -> Result<(), String> {
    state.snippets.delete(&id)
}

/// WAT-105: frontend calls this on mount to render a banner for any
/// non-fatal conditions that surfaced during app setup.
#[tauri::command]
fn get_startup_warnings(state: State<AppState>) -> Vec<StartupWarning> {
    state.startup_warnings.list()
}

/// WAT-105: dismiss a specific warning by id. Returns true if something was
/// removed; dismissing an unknown id is not an error (UI may race backend).
#[tauri::command]
fn dismiss_startup_warning(id: String, state: State<AppState>) -> bool {
    state.startup_warnings.dismiss(&id)
}

// --- WAT-406: notifications drawer ---

#[tauri::command]
fn list_notifications(state: State<AppState>) -> Vec<notifications::Notification> {
    state.notifications.list()
}

#[tauri::command]
fn notifications_unread_count(state: State<AppState>) -> usize {
    state.notifications.unread_count()
}

#[tauri::command]
fn dismiss_notification(id: String, state: State<AppState>) -> bool {
    state.notifications.dismiss(&id)
}

#[tauri::command]
fn dismiss_all_notifications(state: State<AppState>) -> usize {
    state.notifications.dismiss_all()
}

/// WAT-206: open the OS file browser at the config directory. Action
/// target for the "Open config folder" button on the settings-schema
/// warning banner.
#[tauri::command]
fn open_config_folder() -> Result<(), String> {
    let dir = config::get_config_dir().ok_or("Could not determine config directory")?;
    open::that(&dir).map_err(|e| e.to_string())
}

/// macOS-only deep-link to the Accessibility pane in System Settings.
/// Action target for the "Grant access" button on the AX-permission
/// banner. The `x-apple.systempreferences:` scheme has been stable
/// since 10.10; on Linux/Windows this command is a no-op error so
/// the frontend can call it unconditionally without platform checks.
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
        open::that(url).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(String::from(
            "Accessibility permission is a macOS-only concept",
        ))
    }
}

/// Re-probe macOS Accessibility trust. If the user just granted
/// access in System Settings, the AX subsystem updates immediately
/// — no app restart required. On success, also clears the
/// startup-warning banner so the UI reflects the new state without
/// the user manually dismissing it. Returns the new trust state.
#[tauri::command]
fn recheck_accessibility_permission(state: State<AppState>) -> bool {
    #[cfg(target_os = "macos")]
    {
        let trusted = actions::windows::ax_is_trusted();
        if trusted {
            state.startup_warnings.clear_accessibility_permission_denied();
        } else {
            // Re-record (idempotent via fixed id) so a stale dismissal
            // by the user — followed by a deny — re-surfaces the
            // banner.
            state
                .startup_warnings
                .record_accessibility_permission_denied();
        }
        trusted
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        true
    }
}

/// WAT-404: open the OS file browser at `path`'s parent and (where
/// supported) select the file. Used by the per-result Cmd+K menu's
/// "Reveal in folder" action.
///
/// - Windows: `explorer /select,<path>` opens Explorer and highlights
///   the file.
/// - macOS: `open -R <path>` does the same in Finder.
/// - Linux: falls back to opening the parent directory; no portable
///   "select this file" affordance exists across file managers.
#[tauri::command]
fn reveal_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Note: the comma is required AND must be wedged in the same
        // arg as `/select` — separating them as two args fails on
        // Windows. Spawn (not status) so we don't block on Explorer.
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let parent = std::path::Path::new(&path)
            .parent()
            .ok_or_else(|| "path has no parent directory".to_string())?;
        open::that(parent).map_err(|e| e.to_string())
    }
}

/// WAT-205: returns the list of query prefixes the settings UI should flag
/// as unreachable if reused as a web-search keyword.
#[tauri::command]
fn get_reserved_prefixes() -> Vec<&'static str> {
    config::user_reserved_prefixes()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Arc::new(Database::new().expect("Failed to initialize database"));
    let scratchpad = ScratchpadManager::new(Arc::clone(&db));

    // Initialize notes manager
    let notes_path = directories::ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.data_dir().join("notes"))
        .unwrap_or_else(|| std::path::PathBuf::from("./notes"));
    let notes = NotesManager::new(Arc::clone(&db), notes_path);
    let (settings, settings_warning) = config::load_settings();
    let indexer = get_indexer();
    let mut indexed_apps = indexer.index_apps();
    // WAT-201: bring persisted launch stats into the in-memory cache so
    // the very first search after launch already ranks by usage.
    let _ = apps::merge_stats_into_apps(&db, &mut indexed_apps);

    // Initialize clipboard manager
    let clipboard = ClipboardManager::new(50, Arc::clone(&db)); // Store last 50 entries
    // WAT-303: compile the user's ignore-pattern list. Bad regexes
    // surface through the same startup-warnings channel as everything
    // else; the good patterns still load.
    let (ignore_patterns, pattern_errors) =
        clipboard::compile_ignore_patterns(&settings.clipboard.ignore_patterns);
    clipboard.set_ignore_patterns(ignore_patterns);
    clipboard.start_monitoring();

    // Initialize file search manager
    let file_search = Arc::new(FileSearchManager::new(Arc::clone(&db)));

    // WAT-206: settings warnings surface via the same banner infrastructure
    // as WAT-105's shortcut failures. Build the container up-front so we
    // can push before handing it off to `manage()`.
    let startup_warnings = StartupWarnings::new();
    // WAT-406: mirror every startup warning into the notifications
    // drawer so dismissing the loud banner doesn't erase the history.
    let notifications = NotificationsManager::new();
    if let Some(config::SettingsWarning::FromNewerVersion {
        file_version,
        supported_version,
    }) = settings_warning
    {
        startup_warnings.record_settings_from_newer_version(file_version, supported_version);
        notifications.push(
            notifications::Severity::Warning,
            "Settings from a newer Watson",
            &format!(
                "Your config.toml is schema v{file_version}; this binary supports up to v{supported_version}. Running with defaults."
            ),
        );
    }
    // WAT-303: surface ignore-pattern compile errors through the same
    // banner channel. Valid patterns already applied above.
    startup_warnings.record_invalid_clipboard_filters(&pattern_errors);
    if !pattern_errors.is_empty() {
        notifications.push(
            notifications::Severity::Warning,
            "Clipboard filter errors",
            &format!("{} pattern(s) failed to compile: {}", pattern_errors.len(), pattern_errors.join("; ")),
        );
    }

    // macOS only: Accessibility permission is required for the window
    // and browser-tab switcher. Probe non-prompting at startup and
    // surface a banner if denied — the user grants once in System
    // Settings and re-probes via the banner's "Re-check" button
    // without restarting Watson.
    #[cfg(target_os = "macos")]
    if !actions::windows::ax_is_trusted() {
        startup_warnings.record_accessibility_permission_denied();
        notifications.push(
            notifications::Severity::Warning,
            "Accessibility permission needed",
            "Watson needs Accessibility access for the window + browser-tab switcher. \
             Grant it in System Settings → Privacy & Security → Accessibility.",
        );
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            db: Arc::clone(&db),
            search_engine: SearchEngine::new(),
            indexed_apps: RwLock::new(indexed_apps),
            settings: RwLock::new(settings),
            clipboard,
            scratchpad,
            notes,
            file_search,
            snippets: SnippetsManager::new(Arc::clone(&db)),
            startup_warnings,
            notifications,
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Register global shortcut (Alt+Space)
            // WAT-105 / R-11: registration can fail (hotkey already bound
            // by another app, platform restriction, no keyboard daemon on
            // some Linux WMs). Historically `?` here aborted Tauri setup
            // entirely, leaving the user with a window that silently
            // didn't respond to the hotkey. Now: capture the error as a
            // startup warning and keep launching so the user can open the
            // app via the tray or CLI and rebind.
            let shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
            let hotkey_label = "Alt+Space";
            let hotkey_window = window.clone();

            if let Err(e) = app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    if hotkey_window.is_visible().unwrap_or(false) {
                        hotkey_window.hide().ok();
                    } else {
                        hotkey_window.show().ok();
                        hotkey_window.set_focus().ok();
                        // WAT-407: recenter so the launcher reliably
                        // appears mid-screen on whatever monitor the
                        // user is currently looking at.
                        center_window_on_current_monitor(&hotkey_window);
                    }
                }
            }) {
                eprintln!("watson: failed to register hotkey {hotkey_label}: {e}");
                let state: State<AppState> = app.state();
                state
                    .startup_warnings
                    .record_shortcut_unavailable(hotkey_label, &e.to_string());
                // WAT-406: mirror into the drawer so the history
                // persists after the loud banner is dismissed.
                state.notifications.push(
                    notifications::Severity::Warning,
                    "Hotkey unavailable",
                    &format!(
                        "Couldn't register {hotkey_label}: {e}. Another app may be using it."
                    ),
                );
            }

            // Create system tray (macOS and Windows only - Linux requires appindicator)
            #[cfg(not(target_os = "linux"))]
            {
                let quit = MenuItem::with_id(app, "quit", "Quit Watson", true, None::<&str>)?;
                let show = MenuItem::with_id(app, "show", "Show Watson", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &quit])?;

                let _tray = TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .on_menu_event(|app: &tauri::AppHandle, event| match event.id.as_ref() {
                        "quit" => {
                            app.exit(0);
                        }
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        _ => {}
                    })
                    .build(app)?;
            }

            Ok(())
        })
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
            pin_clipboard_entry,
            unpin_clipboard_entry,
            get_scratchpad,
            set_scratchpad,
            clear_scratchpad,
            create_note,
            update_note,
            delete_note,
            get_note,
            reload_note_from_disk,
            search_notes,
            get_recent_notes,
            search_files,
            search_files_by_extension,
            get_recent_files,
            reindex_files,
            cancel_reindex_files,
            clear_file_index,
            list_snippets,
            create_snippet,
            update_snippet,
            delete_snippet,
            list_notifications,
            notifications_unread_count,
            dismiss_notification,
            dismiss_all_notifications,
            get_startup_warnings,
            dismiss_startup_warning,
            open_config_folder,
            open_accessibility_settings,
            recheck_accessibility_permission,
            reveal_file,
            get_reserved_prefixes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
