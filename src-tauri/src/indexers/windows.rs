use crate::db::AppEntry;
use crate::indexers::AppIndexer;
use std::fs;
use std::path::PathBuf;

pub struct WindowsIndexer {
    search_paths: Vec<PathBuf>,
}

impl WindowsIndexer {
    pub fn new() -> Self {
        let program_data = std::env::var("ProgramData").unwrap_or_default();
        let app_data = std::env::var("APPDATA").unwrap_or_default();

        WindowsIndexer {
            search_paths: vec![
                PathBuf::from(format!(
                    "{}\\Microsoft\\Windows\\Start Menu\\Programs",
                    program_data
                )),
                PathBuf::from(format!(
                    "{}\\Microsoft\\Windows\\Start Menu\\Programs",
                    app_data
                )),
            ],
        }
    }

    fn scan_directory(&self, path: &PathBuf) -> Vec<AppEntry> {
        let mut apps = Vec::new();
        self.scan_directory_recursive(path, &mut apps);
        apps
    }

    fn scan_directory_recursive(&self, path: &PathBuf, apps: &mut Vec<AppEntry>) {
        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                self.scan_directory_recursive(&path, apps);
            } else if path.extension().map(|e| e == "lnk").unwrap_or(false) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    // Skip uninstall shortcuts
                    if name.to_lowercase().contains("uninstall") {
                        continue;
                    }

                    let id = format!("app:{}", path.display());
                    apps.push(AppEntry {
                        id,
                        name: name.to_string(),
                        path: path.display().to_string(),
                        icon_cache_path: None,
                        launch_count: 0,
                        last_launched: None,
                        platform: "windows".to_string(),
                    });
                }
            }
        }
    }
}

impl AppIndexer for WindowsIndexer {
    fn index_apps(&self) -> Vec<AppEntry> {
        let mut all_apps = Vec::new();
        for path in &self.search_paths {
            all_apps.extend(self.scan_directory(path));
        }
        all_apps
    }

    fn get_app_icon(&self, _app: &AppEntry) -> Option<Vec<u8>> {
        // TODO: Extract icon from .exe or .lnk
        None
    }
}
