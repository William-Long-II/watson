use crate::db::AppEntry;
use crate::indexers::AppIndexer;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct MacOSIndexer {
    search_paths: Vec<PathBuf>,
}

impl MacOSIndexer {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        MacOSIndexer {
            search_paths: vec![
                PathBuf::from("/Applications"),
                PathBuf::from("/System/Applications"),
                PathBuf::from(format!("{}/Applications", home)),
            ],
        }
    }

    fn scan_directory(&self, path: &PathBuf) -> Vec<AppEntry> {
        let mut apps = Vec::new();

        let entries = match fs::read_dir(path) {
            Ok(e) => e,
            Err(_) => return apps,
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "app").unwrap_or(false) {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let id = format!("app:{}", path.display());
                    apps.push(AppEntry {
                        id,
                        name: name.to_string(),
                        path: path.display().to_string(),
                        icon_cache_path: None,
                        launch_count: 0,
                        last_launched: None,
                        platform: "macos".to_string(),
                    });
                }
            }
        }

        apps
    }
}

impl AppIndexer for MacOSIndexer {
    fn index_apps(&self) -> Vec<AppEntry> {
        let mut all_apps = Vec::new();
        for path in &self.search_paths {
            all_apps.extend(self.scan_directory(path));
        }
        all_apps
    }

    fn get_app_icon(&self, _app: &AppEntry) -> Option<Vec<u8>> {
        // TODO: Extract icon from .app bundle
        None
    }
}
