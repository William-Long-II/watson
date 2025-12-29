pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    icon_cache_path TEXT,
    launch_count INTEGER DEFAULT 0,
    last_launched INTEGER,
    platform TEXT NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS search_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query TEXT NOT NULL,
    selected_item_id TEXT,
    selected_item_type TEXT,
    timestamp INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS icons (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    cache_path TEXT NOT NULL,
    hash TEXT NOT NULL,
    cached_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_apps_name ON apps(name);
CREATE INDEX IF NOT EXISTS idx_history_query ON search_history(query);
CREATE INDEX IF NOT EXISTS idx_history_timestamp ON search_history(timestamp);
"#;
