#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use crate::db::AppEntry;

pub trait AppIndexer: Send + Sync {
    fn index_apps(&self) -> Vec<AppEntry>;
    fn get_app_icon(&self, app: &AppEntry) -> Option<Vec<u8>>;
}

#[cfg(target_os = "macos")]
pub fn get_indexer() -> impl AppIndexer {
    macos::MacOSIndexer::new()
}

#[cfg(target_os = "windows")]
pub fn get_indexer() -> impl AppIndexer {
    windows::WindowsIndexer::new()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn get_indexer() -> impl AppIndexer {
    StubIndexer
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct StubIndexer;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl AppIndexer for StubIndexer {
    fn index_apps(&self) -> Vec<AppEntry> { vec![] }
    fn get_app_icon(&self, _app: &AppEntry) -> Option<Vec<u8>> { None }
}

#[cfg(test)]
mod tests;
