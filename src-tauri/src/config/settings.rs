use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub general: GeneralSettings,
    pub activation: ActivationSettings,
    pub search: SearchSettings,
    pub theme: ThemeSettings,
    #[serde(default)]
    pub web_searches: Vec<WebSearch>,
    #[serde(default)]
    pub file_search: FileSearchSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    #[serde(default = "default_true")]
    pub launch_at_login: bool,
    #[serde(default)]
    pub show_in_dock: bool,
    #[serde(default)]
    pub show_in_taskbar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationSettings {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSettings {
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default = "default_true")]
    pub show_recently_used: bool,
    #[serde(default = "default_threshold")]
    pub fuzzy_match_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    pub custom: Option<CustomTheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTheme {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub border: Option<String>,
    pub selected_background: Option<String>,
    pub input_background: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<u32>,
    pub border_radius: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearch {
    pub name: String,
    pub keyword: String,
    pub url: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub requires_setup: bool,
    #[serde(default)]
    pub instance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSearchSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_indexed_paths")]
    pub indexed_paths: Vec<String>,
    #[serde(default = "default_excluded_patterns")]
    pub excluded_patterns: Vec<String>,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

impl Default for FileSearchSettings {
    fn default() -> Self {
        FileSearchSettings {
            enabled: true,
            indexed_paths: default_indexed_paths(),
            excluded_patterns: default_excluded_patterns(),
            max_depth: default_max_depth(),
        }
    }
}

fn default_indexed_paths() -> Vec<String> {
    vec![
        "~/Documents".to_string(),
        "~/Downloads".to_string(),
        "~/Desktop".to_string(),
    ]
}

fn default_excluded_patterns() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        ".git".to_string(),
        ".cache".to_string(),
        "__pycache__".to_string(),
        "target".to_string(),
        ".DS_Store".to_string(),
    ]
}

fn default_max_depth() -> usize { 5 }

fn default_true() -> bool { true }
fn default_hotkey() -> String { "Alt+Space".to_string() }
fn default_max_results() -> usize { 8 }
fn default_threshold() -> f64 { 0.6 }
fn default_mode() -> String { "system".to_string() }
fn default_accent() -> String { "system".to_string() }

impl Default for Settings {
    fn default() -> Self {
        Settings {
            general: GeneralSettings {
                launch_at_login: true,
                show_in_dock: false,
                show_in_taskbar: false,
            },
            activation: ActivationSettings {
                hotkey: default_hotkey(),
                show_tray_icon: true,
            },
            search: SearchSettings {
                max_results: 8,
                show_recently_used: true,
                fuzzy_match_threshold: 0.6,
            },
            theme: ThemeSettings {
                mode: "system".to_string(),
                accent_color: "system".to_string(),
                custom: None,
            },
            web_searches: default_web_searches(),
            file_search: FileSearchSettings::default(),
        }
    }
}

fn default_web_searches() -> Vec<WebSearch> {
    vec![
        WebSearch {
            name: "Google".to_string(),
            keyword: "g".to_string(),
            url: "https://www.google.com/search?q={query}".to_string(),
            icon: Some("google".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "DuckDuckGo".to_string(),
            keyword: "ddg".to_string(),
            url: "https://duckduckgo.com/?q={query}".to_string(),
            icon: Some("duckduckgo".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "YouTube".to_string(),
            keyword: "yt".to_string(),
            url: "https://www.youtube.com/results?search_query={query}".to_string(),
            icon: Some("youtube".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "GitHub".to_string(),
            keyword: "gh".to_string(),
            url: "https://github.com/search?q={query}".to_string(),
            icon: Some("github".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "Wikipedia".to_string(),
            keyword: "wiki".to_string(),
            url: "https://en.wikipedia.org/wiki/Special:Search?search={query}".to_string(),
            icon: Some("wikipedia".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "Stack Overflow".to_string(),
            keyword: "so".to_string(),
            url: "https://stackoverflow.com/search?q={query}".to_string(),
            icon: Some("stackoverflow".to_string()),
            requires_setup: false,
            instance: None,
        },
        WebSearch {
            name: "Jira".to_string(),
            keyword: "jira".to_string(),
            url: "https://{instance}.atlassian.net/browse/{query}".to_string(),
            icon: Some("jira".to_string()),
            requires_setup: true,
            instance: None,
        },
    ]
}
