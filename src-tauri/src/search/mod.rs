use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub result_type: ResultType,
    pub score: i64,
    pub action: SearchAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultType {
    Application,
    WebSearch,
    SystemCommand,
    Clipboard,
    Note,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchAction {
    LaunchApp { path: String },
    OpenUrl { url: String },
    RunCommand { command: String },
    CopyClipboard { content: String },
    OpenNote { note_id: String },
    OpenFile { path: String },
}

pub struct SearchEngine {
    matcher: SkimMatcherV2,
}

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn score(&self, query: &str, target: &str) -> Option<i64> {
        self.matcher.fuzzy_match(target, query)
    }

    pub fn search(&self, query: &str, items: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut results: Vec<(SearchResult, i64)> = items
            .into_iter()
            .filter_map(|mut item| {
                self.score(query, &item.name).map(|score| {
                    item.score = score;
                    (item, score)
                })
            })
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.into_iter().map(|(item, _)| item).collect()
    }
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
