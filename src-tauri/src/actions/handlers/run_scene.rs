//! Handler for `SearchAction::RunScene` (Phase 2a, #74).
//!
//! A Scene is a composition of existing actions, so this handler owns
//! no launch logic of its own: each `SceneStep` is mapped onto the
//! sibling handler that already serves the matching `SearchAction`
//! (`launch_app`, `open_url`, `focus_window`, `run_command`), and
//! `scenes::run_steps` sequences them with the Scene's inter-step
//! delay.
//!
//! Failed steps never abort the Scene. Once the run finishes, any
//! failures are collected into a single notification so the user sees
//! which steps didn't take without a toast per step.
//!
//! Blocking: this sleeps between steps, so callers must run it off
//! the main thread (see `lib.rs::execute_action`).

use std::sync::RwLock;
use std::time::Duration;

use crate::actions::handlers::{focus_window, launch_app, open_url, run_command};
use crate::actions::windows::get_open_windows;
use crate::db::{AppEntry, Database};
use crate::notifications::{NotificationsManager, Severity};
use crate::scenes::{resolve_window, run_steps, Scene, SceneStep, StepFailure};

pub fn handle(
    scene: &Scene,
    db: &Database,
    indexed_apps: &RwLock<Vec<AppEntry>>,
    notifications: &NotificationsManager,
) {
    let failures = run_steps(
        &scene.steps,
        Duration::from_millis(scene.inter_step_delay_ms as u64),
        |step| run_step(step, db, indexed_apps),
        std::thread::sleep,
    );
    if !failures.is_empty() {
        notifications.push(
            Severity::Warning,
            &format!("Scene \u{201c}{}\u{201d} finished with errors", scene.name),
            &failure_message(&failures, scene.steps.len()),
        );
    }
}

fn run_step(
    step: &SceneStep,
    db: &Database,
    indexed_apps: &RwLock<Vec<AppEntry>>,
) -> Result<(), String> {
    match step {
        SceneStep::LaunchApp { path } => launch_app::handle(path.clone(), db, indexed_apps),
        SceneStep::OpenUrl { url } => open_url::handle(url.clone()),
        SceneStep::FocusWindow { app, title } => {
            // Enumerate at step time, not Scene start: an earlier
            // LaunchApp step may have just created the target window.
            let windows = get_open_windows()?;
            let hwnd = resolve_window(&windows, app, title)
                .ok_or_else(|| "no matching open window".to_string())?;
            focus_window::handle(hwnd)
        }
        SceneStep::RunCommand { command } => run_command::handle(command.clone()),
    }
}

fn failure_message(failures: &[StepFailure], total: usize) -> String {
    let lines: Vec<String> = failures
        .iter()
        .map(|f| format!("Step {} ({}): {}", f.index + 1, f.label, f.error))
        .collect();
    format!(
        "{} of {} steps failed. {}",
        failures.len(),
        total,
        lines.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_message_lists_each_failed_step_one_based() {
        let failures = vec![
            StepFailure {
                index: 0,
                label: "Launch /a.app".into(),
                error: "not found".into(),
            },
            StepFailure {
                index: 3,
                label: "Focus Slack".into(),
                error: "no matching open window".into(),
            },
        ];
        assert_eq!(
            failure_message(&failures, 5),
            "2 of 5 steps failed. Step 1 (Launch /a.app): not found; \
             Step 4 (Focus Slack): no matching open window"
        );
    }
}
