pub mod dispatch;
pub mod url_builder;

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

/// WAT-202 / R-09: description-only matches ranked below name matches.
///
/// When a query matches an item's description but not its name, we still
/// want to surface it (previous behavior silently dropped the item). A
/// 50% multiplier on the description score keeps the relative ordering
/// within description-only hits while pushing them beneath any name hit
/// of comparable quality. The exact value isn't load-bearing — it just
/// has to be <100% so that name-vs-description ties favor the name.
const DESCRIPTION_SCORE_PENALTY_PERCENT: i64 = 50;

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn score(&self, query: &str, target: &str) -> Option<i64> {
        self.matcher.fuzzy_match(target, query)
    }

    /// Score an item against the query, considering both `name` and
    /// `description`. Returns the best of the two (with description
    /// penalized) or `None` if neither matches.
    fn score_item(&self, query: &str, name: &str, description: &str) -> Option<i64> {
        let name_score = self.score(query, name);
        let description_score = self
            .score(query, description)
            .map(|s| s * DESCRIPTION_SCORE_PENALTY_PERCENT / 100);
        // Option::max returns the higher variant; Some(x).max(None) == Some(x).
        name_score.max(description_score)
    }

    pub fn search(&self, query: &str, items: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut results: Vec<(SearchResult, i64)> = items
            .into_iter()
            .filter_map(|mut item| {
                self.score_item(query, &item.name, &item.description).map(|score| {
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
