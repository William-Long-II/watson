//! Handler for `SearchAction::SaveScene` (`scene save <name>`, #74).
//!
//! Enumerates the open windows at the moment the user presses Enter,
//! turns them into `LaunchApp` steps via `scenes::capture`, and stores
//! the result under `name` (replacing the steps of an existing Scene
//! with that name). The outcome — saved, or why not — lands in the
//! notifications drawer, because the launcher has already hidden by
//! the time this runs.

use std::sync::RwLock;

use crate::actions::windows::get_open_windows;
use crate::db::AppEntry;
use crate::notifications::{NotificationsManager, Severity};
use crate::scenes::capture::{capture, summary, Capture};
use crate::scenes::ScenesManager;

pub fn handle(
    name: &str,
    scenes: &ScenesManager,
    indexed_apps: &RwLock<Vec<AppEntry>>,
    notifications: &NotificationsManager,
) -> Result<(), String> {
    let result = get_open_windows().and_then(|windows| {
        let apps = indexed_apps.read().unwrap().clone();
        save(name, capture(&windows, &apps), scenes)
    });
    match &result {
        Ok((title, message)) => notifications.push(Severity::Info, title, message),
        Err(e) => notifications.push(Severity::Error, "Couldn\u{2019}t save Scene", e),
    };
    result.map(|_| ())
}

/// Store `captured` under `name`. Returns the notification title and
/// body on success. Split from `handle` so it runs without a desktop.
fn save(name: &str, captured: Capture, scenes: &ScenesManager) -> Result<(String, String), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Type a name after \u{201c}scene save\u{201d}.".to_string());
    }
    if captured.steps.is_empty() && captured.skipped.is_empty() {
        // Nothing enumerated at all, which on Linux means a Wayland
        // compositor without the wlroots toplevel protocol.
        return Err(
            "Watson couldn\u{2019}t see any open windows. On Linux Wayland this needs \
             a wlroots compositor such as Sway or Hyprland."
                .to_string(),
        );
    }
    if captured.steps.is_empty() {
        return Err(format!(
            "None of the open windows belong to an installed app Watson knows about{}.",
            skipped_suffix(&captured)
        ));
    }
    let message = format!("{}{}", summary(&captured), skipped_suffix(&captured));
    let (scene, replaced) = scenes.save_captured(name, captured.steps)?;
    let verb = if replaced { "Updated" } else { "Saved" };
    Ok((
        format!("{verb} Scene \u{201c}{}\u{201d}", scene.name),
        message,
    ))
}

fn skipped_suffix(captured: &Capture) -> String {
    if captured.skipped.is_empty() {
        String::new()
    } else {
        format!(" (skipped: {})", captured.skipped.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::scenes::SceneStep;
    use std::sync::Arc;

    fn manager() -> ScenesManager {
        ScenesManager::new(Arc::new(Database::in_memory().unwrap()))
    }

    fn captured(apps: &[&str], skipped: &[&str]) -> Capture {
        Capture {
            steps: apps
                .iter()
                .map(|a| SceneStep::LaunchApp {
                    path: format!("/Applications/{a}.app"),
                })
                .collect(),
            app_names: apps.iter().map(|a| a.to_string()).collect(),
            skipped: skipped.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn saves_new_scene_and_reports_skipped_apps() {
        let m = manager();
        let (title, message) =
            save("Focus", captured(&["Slack", "Code"], &["Helper"]), &m).unwrap();
        assert_eq!(title, "Saved Scene “Focus”");
        assert_eq!(message, "2 apps: Slack, Code (skipped: Helper)");
        assert_eq!(m.find_by_name("focus").unwrap().unwrap().steps.len(), 2);
    }

    #[test]
    fn saving_an_existing_name_says_updated() {
        let m = manager();
        save("Focus", captured(&["Slack"], &[]), &m).unwrap();
        let (title, _) = save("focus", captured(&["Code"], &[]), &m).unwrap();
        assert_eq!(title, "Updated Scene “Focus”");
        assert_eq!(m.list().unwrap().len(), 1);
    }

    #[test]
    fn no_windows_at_all_explains_the_wayland_limit() {
        let err = save("Focus", captured(&[], &[]), &manager()).unwrap_err();
        assert!(err.contains("couldn’t see any open windows"), "{err}");
    }

    #[test]
    fn blank_name_is_rejected() {
        assert!(save("  ", captured(&["Slack"], &[]), &manager()).is_err());
    }

    #[test]
    fn nothing_captured_is_rejected_and_names_what_was_skipped() {
        let m = manager();
        let err = save("Focus", captured(&[], &["Helper"]), &m).unwrap_err();
        assert!(err.contains("skipped: Helper"), "{err}");
        assert!(m.list().unwrap().is_empty());
    }
}
