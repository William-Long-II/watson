use super::{FileEntry, FileSearchManager};
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

pub struct FileIndexer {
    manager: Arc<FileSearchManager>,
    indexed_paths: Vec<String>,
    excluded_patterns: Vec<String>,
    max_depth: usize,
}

impl FileIndexer {
    pub fn new(
        manager: Arc<FileSearchManager>,
        indexed_paths: Vec<String>,
        excluded_patterns: Vec<String>,
        max_depth: usize,
    ) -> Self {
        FileIndexer {
            manager,
            indexed_paths,
            excluded_patterns,
            max_depth,
        }
    }

    pub fn index_all(&self) -> usize {
        let mut count = 0;
        for path_str in &self.indexed_paths {
            let path = expand_path(path_str);
            if path.exists() && path.is_dir() {
                count += self.index_directory(&path);
            }
        }
        count
    }

    fn index_directory(&self, dir: &Path) -> usize {
        let mut count = 0;
        let walker = WalkDir::new(dir)
            .max_depth(self.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !self.is_excluded(e.path()));

        for entry in walker.flatten() {
            if entry.file_type().is_file() {
                if let Some(file_entry) = self.create_file_entry(entry.path()) {
                    if self.manager.insert(&file_entry).is_ok() {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    fn is_excluded(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in &self.excluded_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }
        false
    }

    fn create_file_entry(&self, path: &Path) -> Option<FileEntry> {
        let metadata = path.metadata().ok()?;
        let name = path.file_name()?.to_string_lossy().to_string();
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase());
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|| Utc::now().timestamp());

        Some(FileEntry {
            id: format!("file:{}", path.to_string_lossy().replace(['/', '\\'], "-")),
            name,
            path: path.to_string_lossy().to_string(),
            extension,
            size_bytes: Some(metadata.len() as i64),
            modified_at,
        })
    }
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
}
