mod actions;
mod config;
mod db;
mod indexers;
mod search;

use actions::system::{execute_command, get_system_commands};
use config::settings::Settings;
use db::{AppEntry, Database};
use indexers::{get_indexer, AppIndexer};
use search::{ResultType, SearchAction, SearchEngine, SearchResult};
use std::sync::{Arc, RwLock};
use tauri::State;

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
    indexed_apps: RwLock<Vec<AppEntry>>,
    settings: RwLock<Settings>,
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

    // Check for web search keyword match first
    for ws in &settings.web_searches {
        if query.starts_with(&format!("{} ", ws.keyword)) {
            let search_query = query.strip_prefix(&format!("{} ", ws.keyword)).unwrap();
            if !search_query.is_empty() {
                let url = if ws.requires_setup && ws.instance.is_none() {
                    continue; // Skip if requires setup but not configured
                } else if ws.requires_setup {
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
fn execute_action(action: SearchAction) -> Result<(), String> {
    match action {
        SearchAction::LaunchApp { path } => actions::launch_app(&path),
        SearchAction::OpenUrl { url } => actions::open_url(&url),
        SearchAction::RunCommand { command } => execute_command(&command),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Database::new().expect("Failed to initialize database");
    let settings = config::load_settings();
    let indexer = get_indexer();
    let indexed_apps = indexer.index_apps();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
            indexed_apps: RwLock::new(indexed_apps),
            settings: RwLock::new(settings),
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings_cmd,
            reindex_apps,
            search,
            execute_action
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
