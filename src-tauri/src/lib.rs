mod actions;
mod clipboard;
mod config;
mod db;
mod files;
mod indexers;
mod notes;
mod scratchpad;
mod search;
mod warnings;

use actions::system::{execute_command, get_system_commands};
use clipboard::ClipboardManager;
use config::settings::Settings;
use files::{FileEntry, FileSearchManager, indexer::FileIndexer};
use notes::NotesManager;
use scratchpad::ScratchpadManager;
use warnings::{StartupWarning, StartupWarnings};
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
use search::dispatch::{classify_prefix_route, match_web_search, Route, SubQuery, WebSearchMatch};
use search::url_builder::build_web_search_url;
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
    /// WAT-105: non-fatal conditions that surfaced during `setup()` and
    /// should be rendered in the UI — e.g., the global hotkey couldn't be
    /// registered because another app already owns it.
    startup_warnings: StartupWarnings,
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

fn notes_route_results(state: &State<AppState>, sub: SubQuery) -> Vec<SearchResult> {
    let notes = match sub {
        SubQuery::Listing => state.notes.get_recent(8),
        SubQuery::Search(ref q) if q.is_empty() => state.notes.get_recent(8),
        SubQuery::Search(q) => state.notes.search(&q),
    };
    notes
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
                    action: SearchAction::OpenNote { note_id: note.id },
                })
                .collect()
        })
        .unwrap_or_default()
}

fn files_route_results(state: &State<AppState>, sub: SubQuery) -> Vec<SearchResult> {
    let files = match sub {
        SubQuery::Listing => state.file_search.get_recent(8),
        SubQuery::Search(ref q) if q.is_empty() => state.file_search.get_recent(8),
        SubQuery::Search(q) => state.file_search.search(&q, 8),
    };
    files
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
                    action: SearchAction::OpenFile { path: file.path },
                })
                .collect()
        })
        .unwrap_or_default()
}

fn clipboard_route_results(state: &State<AppState>, sub: SubQuery) -> Vec<SearchResult> {
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
            description: format!("Copied {}", entry.timestamp.format("%H:%M:%S")),
            icon: Some("clipboard".to_string()),
            result_type: ResultType::Clipboard,
            score: 10000,
            action: SearchAction::CopyClipboard { content: entry.content },
        })
        .collect()
}

#[tauri::command]
fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    match classify_prefix_route(&query) {
        Route::Empty => return vec![],
        Route::Notes(sub) => return notes_route_results(&state, sub),
        Route::Files(sub) => return files_route_results(&state, sub),
        Route::Clipboard(sub) => return clipboard_route_results(&state, sub),
        Route::Passthrough => {}
    }

    let settings = state.settings.read().unwrap();
    let apps = state.indexed_apps.read().unwrap();

    let mut items: Vec<SearchResult> = Vec::new();

    // Web search keyword match. Classification lives in search::dispatch;
    // URL construction (scheme allowlist + {instance} validation + {query}
    // encoding) lives in search::url_builder. A builder error (bad scheme,
    // bad instance) silently skips the result — same policy as NeedsInstance.
    if let WebSearchMatch::Matched { index, subquery } =
        match_web_search(&query, &settings.web_searches)
    {
        let ws = &settings.web_searches[index];
        if let Ok(url) = build_web_search_url(&ws.url, ws.instance.as_deref(), &subquery) {
            items.push(SearchResult {
                id: format!("web:{}", ws.keyword),
                name: format!("{}: {}", ws.name, subquery),
                description: "Web Search".to_string(),
                icon: ws.icon.clone(),
                result_type: ResultType::WebSearch,
                score: 10000,
                action: SearchAction::OpenUrl { url },
            });
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
fn execute_action(action: SearchAction, state: State<AppState>) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => actions::launch_app(&path),
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
        SearchAction::CopyClipboard { content } => state.clipboard.copy_to_clipboard(&content),
        SearchAction::OpenNote { note_id: _ } => {
            // Note opening is handled by the frontend
            Ok(())
        }
        SearchAction::OpenFile { path } => {
            open::that(&path).map_err(|e| e.to_string())
        }
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
}

#[tauri::command]
fn resize_window(window: tauri::WebviewWindow, height: u32) {
    use tauri::LogicalSize;
    let _ = window.set_size(LogicalSize::new(600, height));
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
fn create_note(title: String, content: String, state: State<AppState>) -> Result<notes::Note, String> {
    state.notes.create(&title, &content)
}

#[tauri::command]
fn update_note(id: String, title: String, content: String, state: State<AppState>) -> Result<notes::Note, String> {
    state.notes.update(&id, &title, &content)
}

#[tauri::command]
fn delete_note(id: String, state: State<AppState>) -> Result<(), String> {
    state.notes.delete(&id)
}

#[tauri::command]
fn get_note(id: String, state: State<AppState>) -> Result<Option<notes::Note>, String> {
    state.notes.get(&id)
}

#[tauri::command]
fn search_notes(query: String, state: State<AppState>) -> Result<Vec<notes::Note>, String> {
    state.notes.search(&query)
}

#[tauri::command]
fn get_recent_notes(limit: usize, state: State<AppState>) -> Result<Vec<notes::Note>, String> {
    state.notes.get_recent(limit)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Arc::new(Database::new().expect("Failed to initialize database"));
    let scratchpad = ScratchpadManager::new(Arc::clone(&db));

    // Initialize notes manager
    let notes_path = directories::ProjectDirs::from("com", "watson", "Watson")
        .map(|dirs| dirs.data_dir().join("notes"))
        .unwrap_or_else(|| std::path::PathBuf::from("./notes"));
    let notes = NotesManager::new(Arc::clone(&db), notes_path);
    let settings = config::load_settings();
    let indexer = get_indexer();
    let indexed_apps = indexer.index_apps();

    // Initialize clipboard manager
    let clipboard = ClipboardManager::new(50); // Store last 50 entries
    clipboard.start_monitoring();

    // Initialize file search manager
    let file_search = Arc::new(FileSearchManager::new(Arc::clone(&db)));

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
            startup_warnings: StartupWarnings::new(),
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
                    }
                }
            }) {
                eprintln!("watson: failed to register hotkey {hotkey_label}: {e}");
                let state: State<AppState> = app.state();
                state
                    .startup_warnings
                    .record_shortcut_unavailable(hotkey_label, &e.to_string());
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
            get_scratchpad,
            set_scratchpad,
            clear_scratchpad,
            create_note,
            update_note,
            delete_note,
            get_note,
            search_notes,
            get_recent_notes,
            search_files,
            search_files_by_extension,
            get_recent_files,
            reindex_files,
            cancel_reindex_files,
            clear_file_index,
            get_startup_warnings,
            dismiss_startup_warning
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
