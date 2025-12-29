mod config;
mod db;
mod indexers;
mod search;

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

    // Convert apps to search results
    let mut items: Vec<SearchResult> = apps
        .iter()
        .map(|app| SearchResult {
            id: app.id.clone(),
            name: app.name.clone(),
            description: "Application".to_string(),
            icon: app.icon_cache_path.clone(),
            result_type: ResultType::Application,
            score: 0,
            action: SearchAction::LaunchApp {
                path: app.path.clone(),
            },
        })
        .collect();

    // Add web search results
    for ws in &settings.web_searches {
        if query.starts_with(&format!("{} ", ws.keyword)) {
            let search_query = query.strip_prefix(&format!("{} ", ws.keyword)).unwrap();
            let url = ws.url.replace("{query}", &urlencoding::encode(search_query));
            items.insert(
                0,
                SearchResult {
                    id: format!("web:{}", ws.keyword),
                    name: format!("{}: {}", ws.name, search_query),
                    description: "Web Search".to_string(),
                    icon: ws.icon.clone(),
                    result_type: ResultType::WebSearch,
                    score: 1000,
                    action: SearchAction::OpenUrl { url },
                },
            );
        }
    }

    // Search and limit results
    let mut results = state.search_engine.search(&query, items);
    results.truncate(settings.search.max_results);
    results
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
            search
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
