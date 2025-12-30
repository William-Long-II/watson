mod actions;
mod clipboard;
mod config;
mod db;
mod indexers;
mod search;

use actions::system::{execute_command, get_system_commands};
use clipboard::ClipboardManager;
use config::settings::Settings;
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
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

    // Check for clipboard search (cb or clip keyword)
    if query.starts_with("cb ") || query.starts_with("clip ") {
        let clip_query = query
            .strip_prefix("cb ")
            .or_else(|| query.strip_prefix("clip "))
            .unwrap_or("");

        let clip_results = if clip_query.is_empty() {
            state.clipboard.get_history()
        } else {
            state.clipboard.search_history(clip_query)
        };

        for entry in clip_results.into_iter().take(8) {
            items.push(SearchResult {
                id: entry.id,
                name: entry.preview.clone(),
                description: format!("Copied {}", entry.timestamp.format("%H:%M:%S")),
                icon: Some("clipboard".to_string()),
                result_type: ResultType::Clipboard,
                score: 10000,
                action: SearchAction::CopyClipboard { content: entry.content },
            });
        }

        return items;
    }

    // Show recent clipboard if just "cb" or "clip"
    if query == "cb" || query == "clip" {
        for entry in state.clipboard.get_history().into_iter().take(8) {
            items.push(SearchResult {
                id: entry.id,
                name: entry.preview.clone(),
                description: format!("Copied {}", entry.timestamp.format("%H:%M:%S")),
                icon: Some("clipboard".to_string()),
                result_type: ResultType::Clipboard,
                score: 10000,
                action: SearchAction::CopyClipboard { content: entry.content },
            });
        }

        return items;
    }

    // Check for web search keyword match first
    for ws in &settings.web_searches {
        if query.starts_with(&format!("{} ", ws.keyword)) {
            let search_query = query.strip_prefix(&format!("{} ", ws.keyword)).unwrap();
            if !search_query.is_empty() {
                // Check if URL needs instance but it's not configured
                let needs_instance = ws.url.contains("{instance}");
                let has_instance = ws.instance.as_ref().map(|s| !s.is_empty()).unwrap_or(false);

                if needs_instance && !has_instance {
                    continue; // Skip if needs instance but not configured
                }

                let url = if needs_instance {
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
fn execute_action(action: SearchAction, state: State<AppState>) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => actions::launch_app(&path),
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
        SearchAction::CopyClipboard { content } => state.clipboard.copy_to_clipboard(&content),
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Database::new().expect("Failed to initialize database");
    let settings = config::load_settings();
    let indexer = get_indexer();
    let indexed_apps = indexer.index_apps();

    // Initialize clipboard manager
    let clipboard = ClipboardManager::new(50); // Store last 50 entries
    clipboard.start_monitoring();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
            indexed_apps: RwLock::new(indexed_apps),
            settings: RwLock::new(settings),
            clipboard,
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Register global shortcut (Alt+Space)
            let shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
            let hotkey_window = window.clone();

            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    if hotkey_window.is_visible().unwrap_or(false) {
                        hotkey_window.hide().ok();
                    } else {
                        hotkey_window.show().ok();
                        hotkey_window.set_focus().ok();
                    }
                }
            })?;

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
            copy_to_clipboard
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
