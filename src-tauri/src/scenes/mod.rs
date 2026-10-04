//! Phase 2a (#74): Scenes — a named, ordered list of actions that
//! activates with one keystroke ("Start Work").
//!
//! A Scene is a *composition* of the existing action handlers, not a
//! parallel launch pipeline. Each `SceneStep` maps onto one handler in
//! `actions::handlers`; the runner here only sequences them with a
//! small inter-step delay and collects per-step failures so one broken
//! step (app uninstalled, malformed URL) never aborts the rest.
//!
//! ## Persistence
//!
//! Scenes live in the `scenes` SQLite table (migration 007) next to
//! snippets. Steps are stored as a JSON array in `steps_json` — the
//! step list is always read and written as a whole, so a child table
//! would add joins without buying any query we need.
//!
//! ## Why `FocusWindow` stores app + title, not HWND
//!
//! A window handle is only valid for the lifetime of that window, so a
//! Scene saved today would point at nothing tomorrow. Steps record the
//! process name and a title substring instead; the runner resolves
//! them to a live window at activation time via `resolve_window`.

use crate::actions::windows::WindowEntry;
use crate::db::Database;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Default pause between steps. Long enough that a just-launched app
/// has started registering its window before the next step tries to
/// focus something; short enough that a five-step Scene still feels
/// like one action.
pub const DEFAULT_INTER_STEP_DELAY_MS: u32 = 200;

/// Upper bound on the configurable delay. Guards against a typo
/// (`20000` for `200`) turning a Scene into a minute-long stall.
pub const MAX_INTER_STEP_DELAY_MS: u32 = 10_000;

/// One action inside a Scene. V1 supports exactly the four action
/// types DESIGN.md lists; no conditionals, loops or inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SceneStep {
    LaunchApp { path: String },
    OpenUrl { url: String },
    /// Focus the first open window whose process name matches `app`
    /// (case-insensitive, exact) and whose title contains `title`
    /// (case-insensitive substring; empty matches any window).
    FocusWindow { app: String, title: String },
    /// A system command id from `actions::system` (e.g. `lock`).
    RunCommand { command: String },
}

impl SceneStep {
    /// Short human label used in failure notifications.
    pub fn label(&self) -> String {
        match self {
            SceneStep::LaunchApp { path } => format!("Launch {path}"),
            SceneStep::OpenUrl { url } => format!("Open {url}"),
            SceneStep::FocusWindow { app, title } if title.is_empty() => format!("Focus {app}"),
            SceneStep::FocusWindow { app, title } => format!("Focus {app} \u{2014} {title}"),
            SceneStep::RunCommand { command } => format!("Run {command}"),
        }
    }

    /// Reject steps that can never succeed, so the editor surfaces the
    /// problem at save time instead of the runner at activation time.
    fn validate(&self) -> Result<(), String> {
        let (field, value) = match self {
            SceneStep::LaunchApp { path } => ("app path", path),
            SceneStep::OpenUrl { url } => ("URL", url),
            SceneStep::FocusWindow { app, .. } => ("window app", app),
            SceneStep::RunCommand { command } => ("command", command),
        };
        if value.trim().is_empty() {
            return Err(format!("step is missing its {field}"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    /// `scene:<millis>`.
    pub id: String,
    /// Display + search name, e.g. "Start Work".
    pub name: String,
    /// Optional emoji or named icon.
    pub icon: Option<String>,
    pub steps: Vec<SceneStep>,
    pub inter_step_delay_ms: u32,
    pub created_at: i64,
    pub modified_at: i64,
}

/// Fields the user edits. Separate from `Scene` so create/update don't
/// take server-owned fields (id, timestamps) from the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneInput {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub steps: Vec<SceneStep>,
    #[serde(default = "default_delay")]
    pub inter_step_delay_ms: u32,
}

fn default_delay() -> u32 {
    DEFAULT_INTER_STEP_DELAY_MS
}

impl SceneInput {
    /// Trim + validate. Returns the normalized input that gets stored.
    fn normalized(&self) -> Result<SceneInput, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("scene name is required".into());
        }
        if self.steps.is_empty() {
            return Err("a scene needs at least one step".into());
        }
        for (i, step) in self.steps.iter().enumerate() {
            step.validate().map_err(|e| format!("step {}: {e}", i + 1))?;
        }
        let icon = self
            .icon
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        Ok(SceneInput {
            name,
            icon,
            steps: self.steps.clone(),
            inter_step_delay_ms: self.inter_step_delay_ms.min(MAX_INTER_STEP_DELAY_MS),
        })
    }
}

pub struct ScenesManager {
    db: Arc<Database>,
}

impl ScenesManager {
    pub fn new(db: Arc<Database>) -> Self {
        ScenesManager { db }
    }

    pub fn create(&self, input: &SceneInput) -> Result<Scene, String> {
        let input = input.normalized()?;
        let id = format!("scene:{}", Utc::now().timestamp_millis());
        let now = Utc::now().timestamp();
        let steps_json = serde_json::to_string(&input.steps).map_err(|e| e.to_string())?;
        self.db
            .execute(
                "INSERT INTO scenes (id, name, icon, steps_json, inter_step_delay_ms, created_at, modified_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                &[
                    &id,
                    &input.name,
                    &input.icon,
                    &steps_json,
                    &input.inter_step_delay_ms,
                    &now,
                    &now,
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(Scene {
            id,
            name: input.name,
            icon: input.icon,
            steps: input.steps,
            inter_step_delay_ms: input.inter_step_delay_ms,
            created_at: now,
            modified_at: now,
        })
    }

    pub fn update(&self, id: &str, input: &SceneInput) -> Result<Scene, String> {
        let input = input.normalized()?;
        let now = Utc::now().timestamp();
        let steps_json = serde_json::to_string(&input.steps).map_err(|e| e.to_string())?;
        let changed = self
            .db
            .execute(
                "UPDATE scenes SET name = ?, icon = ?, steps_json = ?, inter_step_delay_ms = ?, modified_at = ?
                 WHERE id = ?",
                &[
                    &input.name,
                    &input.icon,
                    &steps_json,
                    &input.inter_step_delay_ms,
                    &now,
                    &id,
                ],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(format!("scene '{id}' not found"));
        }
        self.get(id)?
            .ok_or_else(|| format!("scene '{id}' not found after update"))
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.db
            .execute("DELETE FROM scenes WHERE id = ?", &[&id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Scene>, String> {
        let rows = self
            .db
            .query_map(
                "SELECT id, name, icon, steps_json, inter_step_delay_ms, created_at, modified_at
                 FROM scenes WHERE id = ?",
                &[&id],
                row_to_scene,
            )
            .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().next())
    }

    pub fn list(&self) -> Result<Vec<Scene>, String> {
        self.db
            .query_map(
                "SELECT id, name, icon, steps_json, inter_step_delay_ms, created_at, modified_at
                 FROM scenes ORDER BY LOWER(name) ASC",
                &[],
                row_to_scene,
            )
            .map_err(|e| e.to_string())
    }
}

fn row_to_scene(row: &rusqlite::Row<'_>) -> rusqlite::Result<Scene> {
    let steps_json: String = row.get(3)?;
    // A row we wrote ourselves always parses. If a future schema adds a
    // step type and the user downgrades, surface the Scene with no
    // steps rather than failing the whole list query.
    let steps = serde_json::from_str(&steps_json).unwrap_or_default();
    Ok(Scene {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        steps,
        inter_step_delay_ms: row.get(4)?,
        created_at: row.get(5)?,
        modified_at: row.get(6)?,
    })
}

/// Resolve a `FocusWindow` step to a live window handle. Process name
/// must match exactly (case-insensitive, `.exe` suffix ignored so a
/// Scene authored on Windows reads naturally); title is a
/// case-insensitive substring and an empty title matches any window
/// of that app.
pub fn resolve_window(windows: &[WindowEntry], app: &str, title: &str) -> Option<i64> {
    let normalize = |s: &str| {
        let lower = s.trim().to_lowercase();
        lower.strip_suffix(".exe").map(str::to_string).unwrap_or(lower)
    };
    let app = normalize(app);
    let title = title.trim().to_lowercase();
    windows
        .iter()
        .find(|w| normalize(&w.process_name) == app && w.title.to_lowercase().contains(&title))
        .map(|w| w.hwnd)
}

/// A step that failed during activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepFailure {
    /// 0-based position of the step in the Scene.
    pub index: usize,
    pub label: String,
    pub error: String,
}

/// Run `steps` in order through `exec`, sleeping `delay` between steps
/// (not before the first or after the last). A failing step is
/// recorded and the run continues — the acceptance criteria require
/// that one broken step never aborts the Scene.
///
/// `exec` and `sleep` are injected so the sequencing contract is unit-
/// testable without launching real apps or waiting real time.
pub fn run_steps<E, S>(steps: &[SceneStep], delay: Duration, mut exec: E, mut sleep: S) -> Vec<StepFailure>
where
    E: FnMut(&SceneStep) -> Result<(), String>,
    S: FnMut(Duration),
{
    let mut failures = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        if index > 0 && !delay.is_zero() {
            sleep(delay);
        }
        if let Err(error) = exec(step) {
            failures.push(StepFailure {
                index,
                label: step.label(),
                error,
            });
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> ScenesManager {
        ScenesManager::new(Arc::new(Database::in_memory().expect("in-memory db")))
    }

    fn start_work() -> SceneInput {
        SceneInput {
            name: "Start Work".into(),
            icon: Some("💼".into()),
            steps: vec![
                SceneStep::LaunchApp {
                    path: "/Applications/Visual Studio Code.app".into(),
                },
                SceneStep::LaunchApp {
                    path: "/Applications/Slack.app".into(),
                },
                SceneStep::LaunchApp {
                    path: r"C:\Program Files\Mozilla Firefox\firefox.exe".into(),
                },
                SceneStep::OpenUrl {
                    url: "https://github.com/notifications".into(),
                },
                SceneStep::OpenUrl {
                    url: "https://calendar.google.com".into(),
                },
                SceneStep::FocusWindow {
                    app: "Code".into(),
                    title: "watson".into(),
                },
                SceneStep::RunCommand {
                    command: "maximize".into(),
                },
            ],
            inter_step_delay_ms: 250,
        }
    }

    fn window(hwnd: i64, process_name: &str, title: &str) -> WindowEntry {
        WindowEntry {
            hwnd,
            pid: 1,
            process_name: process_name.into(),
            title: title.into(),
        }
    }

    // --- model / serde ---

    #[test]
    fn step_serializes_with_snake_case_type_tag() {
        let json = serde_json::to_value(SceneStep::FocusWindow {
            app: "Code".into(),
            title: "watson".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "focus_window", "app": "Code", "title": "watson" })
        );
    }

    #[test]
    fn steps_round_trip_through_json() {
        let steps = start_work().steps;
        let json = serde_json::to_string(&steps).unwrap();
        let back: Vec<SceneStep> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, steps);
    }

    #[test]
    fn input_defaults_delay_when_omitted() {
        let input: SceneInput = serde_json::from_value(serde_json::json!({
            "name": "x",
            "steps": [{ "type": "open_url", "url": "https://a.example" }]
        }))
        .unwrap();
        assert_eq!(input.inter_step_delay_ms, DEFAULT_INTER_STEP_DELAY_MS);
        assert_eq!(input.icon, None);
    }

    #[test]
    fn normalize_rejects_blank_name() {
        let mut input = start_work();
        input.name = "   ".into();
        assert!(input.normalized().is_err());
    }

    #[test]
    fn normalize_rejects_empty_steps() {
        let mut input = start_work();
        input.steps.clear();
        assert!(input.normalized().is_err());
    }

    #[test]
    fn normalize_rejects_step_with_blank_target_and_names_it() {
        let mut input = start_work();
        input.steps.push(SceneStep::OpenUrl { url: " ".into() });
        let err = input.normalized().unwrap_err();
        assert!(err.contains("step 8"), "error should name the step: {err}");
    }

    #[test]
    fn normalize_trims_name_drops_blank_icon_and_clamps_delay() {
        let mut input = start_work();
        input.name = "  Start Work  ".into();
        input.icon = Some("  ".into());
        input.inter_step_delay_ms = 999_999;
        let n = input.normalized().unwrap();
        assert_eq!(n.name, "Start Work");
        assert_eq!(n.icon, None);
        assert_eq!(n.inter_step_delay_ms, MAX_INTER_STEP_DELAY_MS);
    }

    // --- persistence ---

    #[test]
    fn create_then_get_round_trips_every_field() {
        let m = manager();
        let created = m.create(&start_work()).unwrap();
        assert!(created.id.starts_with("scene:"));
        let fetched = m.get(&created.id).unwrap().expect("scene exists");
        assert_eq!(fetched, created);
        assert_eq!(fetched.steps.len(), 7);
        assert_eq!(fetched.inter_step_delay_ms, 250);
        assert_eq!(fetched.icon.as_deref(), Some("💼"));
    }

    #[test]
    fn list_is_sorted_by_name_case_insensitively() {
        let m = manager();
        for name in ["zeta", "Alpha", "beta"] {
            let mut input = start_work();
            input.name = name.into();
            m.create(&input).unwrap();
            // ids are millisecond timestamps; keep them distinct.
            std::thread::sleep(Duration::from_millis(2));
        }
        let names: Vec<String> = m.list().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["Alpha", "beta", "zeta"]);
    }

    #[test]
    fn update_replaces_name_steps_and_delay() {
        let m = manager();
        let created = m.create(&start_work()).unwrap();
        let edited = SceneInput {
            name: "Wind Down".into(),
            icon: None,
            steps: vec![SceneStep::RunCommand {
                command: "lock".into(),
            }],
            inter_step_delay_ms: 0,
        };
        let updated = m.update(&created.id, &edited).unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "Wind Down");
        assert_eq!(updated.icon, None);
        assert_eq!(updated.steps, edited.steps);
        assert_eq!(updated.inter_step_delay_ms, 0);
        assert_eq!(updated.created_at, created.created_at);
    }

    #[test]
    fn update_unknown_id_is_an_error() {
        let m = manager();
        assert!(m.update("scene:missing", &start_work()).is_err());
    }

    #[test]
    fn invalid_input_is_not_persisted() {
        let m = manager();
        let mut input = start_work();
        input.steps.clear();
        assert!(m.create(&input).is_err());
        assert!(m.list().unwrap().is_empty());
    }

    #[test]
    fn delete_removes_scene() {
        let m = manager();
        let created = m.create(&start_work()).unwrap();
        m.delete(&created.id).unwrap();
        assert!(m.get(&created.id).unwrap().is_none());
        assert!(m.list().unwrap().is_empty());
    }

    #[test]
    fn scenes_persist_across_reopen() {
        // Acceptance: "Scenes persist across app restarts." Re-open the
        // same on-disk DB file and read the Scene back.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("watson.db");
        let id = {
            let m = ScenesManager::new(Arc::new(Database::new_with_path(&path).unwrap()));
            m.create(&start_work()).unwrap().id
        };
        let m = ScenesManager::new(Arc::new(Database::new_with_path(&path).unwrap()));
        let scene = m.get(&id).unwrap().expect("scene survives reopen");
        assert_eq!(scene.name, "Start Work");
        assert_eq!(scene.steps, start_work().steps);
    }

    #[test]
    fn unparseable_steps_json_degrades_to_empty_steps() {
        let db = Arc::new(Database::in_memory().unwrap());
        db.execute(
            "INSERT INTO scenes (id, name, icon, steps_json, inter_step_delay_ms, created_at, modified_at)
             VALUES ('scene:bad', 'Bad', NULL, '[{\"type\":\"teleport\"}]', 200, 1, 1)",
            &[],
        )
        .unwrap();
        let m = ScenesManager::new(db);
        let scenes = m.list().unwrap();
        assert_eq!(scenes.len(), 1);
        assert!(scenes[0].steps.is_empty());
    }

    // --- window resolution ---

    #[test]
    fn resolve_window_matches_app_case_insensitively_and_title_substring() {
        let wins = vec![
            window(1, "firefox", "Inbox — Mozilla Firefox"),
            window(2, "Code", "lib.rs — watson — Visual Studio Code"),
            window(3, "Code", "main.rs — other — Visual Studio Code"),
        ];
        assert_eq!(resolve_window(&wins, "code", "WATSON"), Some(2));
        assert_eq!(resolve_window(&wins, "Code", ""), Some(2));
        assert_eq!(resolve_window(&wins, "slack", ""), None);
        assert_eq!(resolve_window(&wins, "Code", "nope"), None);
    }

    #[test]
    fn resolve_window_ignores_exe_suffix_either_side() {
        let wins = vec![window(7, "slack.exe", "Slack | general")];
        assert_eq!(resolve_window(&wins, "Slack", ""), Some(7));
        let wins = vec![window(8, "Slack", "Slack | general")];
        assert_eq!(resolve_window(&wins, "slack.exe", ""), Some(8));
    }

    // --- runner ---

    #[test]
    fn run_steps_executes_in_order_with_delay_between_steps_only() {
        let steps = start_work().steps;
        let mut log: Vec<String> = Vec::new();
        let sleeps = std::cell::Cell::new(0);
        let failures = run_steps(
            &steps,
            Duration::from_millis(250),
            |s| {
                log.push(s.label());
                Ok(())
            },
            |d| {
                assert_eq!(d, Duration::from_millis(250));
                sleeps.set(sleeps.get() + 1);
            },
        );
        assert!(failures.is_empty());
        let expected: Vec<String> = steps.iter().map(SceneStep::label).collect();
        assert_eq!(log, expected);
        assert_eq!(sleeps.get(), steps.len() - 1);
    }

    #[test]
    fn run_steps_continues_past_failures_and_reports_each() {
        let steps = vec![
            SceneStep::LaunchApp { path: "/missing.app".into() },
            SceneStep::OpenUrl { url: "https://ok.example".into() },
            SceneStep::RunCommand { command: "bogus".into() },
        ];
        let mut ran = 0;
        let failures = run_steps(
            &steps,
            Duration::ZERO,
            |s| {
                ran += 1;
                match s {
                    SceneStep::OpenUrl { .. } => Ok(()),
                    _ => Err("boom".into()),
                }
            },
            |_| panic!("zero delay must not sleep"),
        );
        assert_eq!(ran, 3, "every step runs even after a failure");
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].index, 0);
        assert_eq!(failures[0].label, "Launch /missing.app");
        assert_eq!(failures[1].index, 2);
        assert_eq!(failures[1].error, "boom");
    }
}
