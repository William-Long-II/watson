#[cfg(test)]
mod tests {
    use crate::indexers::get_indexer;
    use crate::db::AppEntry;
    use crate::indexers::AppIndexer;

    #[test]
    fn test_indexer_trait_exists() {
        // Verify the indexer can be created
        let _indexer = get_indexer();
    }

    #[test]
    fn test_indexer_returns_apps() {
        let indexer = get_indexer();
        let apps = indexer.index_apps();

        // Should return some apps (may be empty in test environments)
        // Just verify it doesn't panic
        let _ = apps.len();
    }

    #[test]
    fn test_app_entry_fields() {
        let entry = AppEntry {
            id: "test:app".to_string(),
            name: "Test App".to_string(),
            path: "/path/to/app".to_string(),
            icon_cache_path: None,
            launch_count: 0,
            last_launched: None,
            platform: "test".to_string(),
        };

        assert_eq!(entry.name, "Test App");
        assert!(entry.id.starts_with("test:"));
    }
}
