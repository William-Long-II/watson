mod config;
mod db;
mod search;

use config::settings::Settings;
use db::Database;
use search::{SearchEngine, SearchResult};
use std::sync::Arc;
use tauri::State;

struct AppState {
    db: Arc<Database>,
    search_engine: SearchEngine,
}

#[tauri::command]
fn get_settings() -> Settings {
    config::load_settings()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    config::save_settings(&settings)
}

#[tauri::command]
fn search(query: String, state: State<AppState>) -> Vec<SearchResult> {
    // TODO: Gather items from indexers and web searches
    let items = vec![];
    state.search_engine.search(&query, items)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            db: Arc::new(db),
            search_engine: SearchEngine::new(),
        })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
