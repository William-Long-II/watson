//! Scenes provider (Phase 2a, #74).
//!
//! Surfaces every saved Scene as a passthrough candidate so typing
//! "start work" or just "work" finds the "Start Work" Scene; Enter
//! fires `SearchAction::RunScene`. Filtering is left to the shared
//! fuzzy `SearchEngine` pass in `lib.rs::search` — users keep a
//! handful of Scenes, so handing all of them to the engine is cheaper
//! than a second matcher here.
//!
//! Lifetime: borrows `&ScenesManager` for one call's duration.

use crate::scenes::{Scene, ScenesManager};
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Pre-filter score. The engine re-scores against the query, so this
/// only matters for callers that skip the fuzzy pass.
const SCENE_SCORE: i64 = 9_000;

/// "3 steps · Launch Code, Open https://…, …" — enough for the user
/// to recognise which Scene they're about to fire.
fn scene_description(scene: &Scene) -> String {
    let count = scene.steps.len();
    let noun = if count == 1 { "step" } else { "steps" };
    let labels: Vec<String> = scene.steps.iter().take(3).map(|s| s.label()).collect();
    let mut summary = labels.join(", ");
    if count > 3 {
        summary.push_str(", \u{2026}");
    }
    if summary.is_empty() {
        format!("Scene \u{b7} {count} {noun}")
    } else {
        format!("Scene \u{b7} {count} {noun} \u{b7} {summary}")
    }
}

pub fn scene_result(scene: Scene) -> SearchResult {
    let name = match scene.icon.as_deref() {
        Some(icon) => format!("{icon}  {}", scene.name),
        None => scene.name.clone(),
    };
    SearchResult {
        id: scene.id.clone(),
        name,
        description: scene_description(&scene),
        icon: Some("scene".to_string()),
        result_type: ResultType::Scene,
        score: SCENE_SCORE,
        frecency_score: 0.0,
        preview: None,
        pinned: false,
        action: SearchAction::RunScene { scene_id: scene.id },
    }
}

pub struct ScenesProvider<'a> {
    pub manager: &'a ScenesManager,
}

#[async_trait::async_trait]
impl<'a> ResultProvider for ScenesProvider<'a> {
    fn name(&self) -> &'static str {
        "scenes"
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        // DB errors read as "no Scenes" — same policy as snippets.
        self.manager
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.steps.is_empty())
            .map(scene_result)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::scenes::{SceneInput, SceneStep};
    use crate::search::SearchEngine;
    use std::sync::Arc;

    fn manager_with(names: &[&str]) -> ScenesManager {
        let m = ScenesManager::new(Arc::new(Database::in_memory().unwrap()));
        for name in names {
            m.create(&SceneInput {
                name: name.to_string(),
                icon: None,
                steps: vec![SceneStep::OpenUrl {
                    url: "https://example.com".into(),
                }],
                inter_step_delay_ms: 200,
            })
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        m
    }

    #[tokio::test]
    async fn start_work_query_finds_the_scene_after_fuzzy_pass() {
        let m = manager_with(&["Start Work", "Movie Night"]);
        let items = ScenesProvider { manager: &m }.search("start work").await;
        let results = SearchEngine::new().search("start work", items);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Start Work");
        assert!(matches!(results[0].result_type, ResultType::Scene));
        assert!(matches!(
            &results[0].action,
            SearchAction::RunScene { scene_id } if scene_id.starts_with("scene:")
        ));
    }

    #[tokio::test]
    async fn partial_word_query_matches() {
        let m = manager_with(&["Start Work"]);
        let items = ScenesProvider { manager: &m }.search("work").await;
        let results = SearchEngine::new().search("work", items);
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn empty_query_returns_nothing() {
        let m = manager_with(&["Start Work"]);
        assert!(ScenesProvider { manager: &m }.search("").await.is_empty());
    }

    #[test]
    fn description_counts_steps_and_previews_first_three() {
        let scene = Scene {
            id: "scene:1".into(),
            name: "x".into(),
            icon: None,
            steps: vec![
                SceneStep::LaunchApp { path: "Code".into() },
                SceneStep::OpenUrl { url: "https://a".into() },
                SceneStep::RunCommand { command: "lock".into() },
                SceneStep::RunCommand { command: "sleep".into() },
            ],
            inter_step_delay_ms: 200,
            created_at: 0,
            modified_at: 0,
        };
        assert_eq!(
            scene_description(&scene),
            "Scene · 4 steps · Launch Code, Open https://a, Run lock, …"
        );
    }

    #[test]
    fn icon_prefixes_the_display_name() {
        let scene = Scene {
            id: "scene:1".into(),
            name: "Start Work".into(),
            icon: Some("💼".into()),
            steps: vec![SceneStep::OpenUrl { url: "https://a".into() }],
            inter_step_delay_ms: 200,
            created_at: 0,
            modified_at: 0,
        };
        assert_eq!(scene_result(scene).name, "💼  Start Work");
    }
}
