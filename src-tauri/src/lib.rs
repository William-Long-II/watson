mod config;

use config::settings::Settings;

#[tauri::command]
fn get_settings() -> Settings {
    config::load_settings()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    config::save_settings(&settings)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![get_settings, save_settings])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
