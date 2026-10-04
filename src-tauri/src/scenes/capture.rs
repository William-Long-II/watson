//! `scene save <name>` (#74): capture the apps that are open right now
//! as the steps of a new Scene.
//!
//! Capture is best-effort by design. Each open window's owning process
//! is matched against the indexed app list, and every distinct app
//! that resolves to an installed app becomes a `LaunchApp` step, in
//! the order its first window appears in the window list. Apps that
//! don't resolve (helper processes, apps outside the indexed folders)
//! are reported back as skipped so the user knows what was left out
//! and can add it in Settings → Scenes.
//!
//! Browser URLs are not captured: tab enumeration only exposes tab
//! titles, not addresses, on every platform we support. Users add
//! `OpenUrl` steps by hand after saving.
//!
//! Pure logic — windows and apps are passed in, so the matching rules
//! are unit-testable without a desktop session.

use crate::actions::windows::WindowEntry;
use crate::db::AppEntry;
use crate::scenes::SceneStep;
use std::path::Path;

/// What a capture produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    /// One `LaunchApp` per distinct captured app, in window order.
    pub steps: Vec<SceneStep>,
    /// Display names of the captured apps, parallel to `steps`.
    pub app_names: Vec<String>,
    /// Process names of open windows that matched no installed app.
    pub skipped: Vec<String>,
}

/// Lower-case, trim, and drop a platform suffix so "Code.exe",
/// "Visual Studio Code.app" and "firefox.desktop" compare by stem.
fn normalize(s: &str) -> String {
    let lower = s.trim().to_lowercase();
    for suffix in [".exe", ".app", ".lnk", ".desktop"] {
        if let Some(stem) = lower.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    lower
}

/// File stem of an app path: "C:\…\Code.exe" → "code",
/// "/Applications/Slack.app" → "slack".
fn path_stem(path: &str) -> String {
    // `Path` splits on the host separator only; normalise so a Windows
    // path in a test (or a synced DB) still yields its file name.
    let unified = path.replace('\\', "/");
    let file = Path::new(&unified)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(&unified);
    normalize(file)
}

/// Find the installed app that owns a process. A match on the app's
/// display name wins over a match on its executable / bundle stem, so
/// "Slack" picks the Slack app even if some other entry's binary is
/// also called `slack`.
pub fn match_app<'a>(process_name: &str, apps: &'a [AppEntry]) -> Option<&'a AppEntry> {
    let process = normalize(process_name);
    if process.is_empty() || process == "unknown" {
        return None;
    }
    apps.iter()
        .find(|a| normalize(&a.name) == process)
        .or_else(|| apps.iter().find(|a| path_stem(&a.path) == process))
}

pub fn capture(windows: &[WindowEntry], apps: &[AppEntry]) -> Capture {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Capture {
        steps: Vec::new(),
        app_names: Vec::new(),
        skipped: Vec::new(),
    };
    for window in windows {
        let key = normalize(&window.process_name);
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        match match_app(&window.process_name, apps) {
            // Two processes can resolve to the same app (a browser and
            // its helper); one launch step is enough.
            Some(app)
                if out
                    .steps
                    .iter()
                    .any(|s| matches!(s, SceneStep::LaunchApp { path } if path == &app.path)) => {}
            Some(app) => {
                out.steps.push(SceneStep::LaunchApp {
                    path: app.path.clone(),
                });
                out.app_names.push(app.name.clone());
            }
            None => out.skipped.push(window.process_name.clone()),
        }
    }
    out
}

/// One-line summary for the search row and the saved notification:
/// "3 apps: Code, Slack, Firefox".
pub fn summary(capture: &Capture) -> String {
    let count = capture.app_names.len();
    let noun = if count == 1 { "app" } else { "apps" };
    if count == 0 {
        return "No open apps match an installed app".to_string();
    }
    format!("{count} {noun}: {}", capture.app_names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: i64, process_name: &str) -> WindowEntry {
        WindowEntry {
            hwnd,
            pid: hwnd as u32,
            process_name: process_name.into(),
            title: format!("{process_name} window"),
        }
    }

    fn app(name: &str, path: &str) -> AppEntry {
        AppEntry {
            id: path.into(),
            name: name.into(),
            path: path.into(),
            icon_cache_path: None,
            launch_count: 0,
            last_launched: None,
            platform: "test".into(),
            modified_at: 0,
        }
    }

    fn installed() -> Vec<AppEntry> {
        vec![
            app(
                "Visual Studio Code",
                r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe",
            ),
            app("Slack", "/Applications/Slack.app"),
            app("Firefox", "/usr/share/applications/firefox.desktop"),
            app("Brave Browser", "/Applications/Brave Browser.app"),
        ]
    }

    #[test]
    fn matches_by_display_name_case_insensitively() {
        let apps = installed();
        assert_eq!(match_app("slack", &apps).unwrap().name, "Slack");
        assert_eq!(match_app("FIREFOX", &apps).unwrap().name, "Firefox");
        assert_eq!(
            match_app("Brave Browser", &apps).unwrap().name,
            "Brave Browser"
        );
    }

    #[test]
    fn falls_back_to_executable_stem() {
        // Windows reports the exe basename ("Code"), not the app name.
        let apps = installed();
        assert_eq!(match_app("Code", &apps).unwrap().name, "Visual Studio Code");
        assert_eq!(
            match_app("Code.exe", &apps).unwrap().name,
            "Visual Studio Code"
        );
    }

    #[test]
    fn unknown_and_unmatched_processes_do_not_match() {
        let apps = installed();
        assert!(match_app("unknown", &apps).is_none());
        assert!(match_app("", &apps).is_none());
        assert!(match_app("SomeHelper", &apps).is_none());
    }

    #[test]
    fn display_name_match_beats_stem_match() {
        let apps = vec![
            app("Slack Helper Tool", "/opt/tools/slack"),
            app("Slack", "/Applications/Slack.app"),
        ];
        assert_eq!(
            match_app("slack", &apps).unwrap().path,
            "/Applications/Slack.app"
        );
    }

    #[test]
    fn capture_dedupes_apps_and_keeps_first_window_order() {
        let windows = vec![
            window(1, "Code"),
            window(2, "Slack"),
            window(3, "Code"),
            window(4, "firefox"),
            window(5, "SomeHelper"),
        ];
        let c = capture(&windows, &installed());
        assert_eq!(c.app_names, vec!["Visual Studio Code", "Slack", "Firefox"]);
        assert_eq!(
            c.steps[0],
            SceneStep::LaunchApp {
                path: r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".into()
            }
        );
        assert_eq!(c.steps.len(), 3);
        assert_eq!(c.skipped, vec!["SomeHelper"]);
    }

    #[test]
    fn two_processes_resolving_to_one_app_yield_one_step() {
        let apps = vec![app("Code", "/usr/bin/code")];
        // "code" by name, "Code.exe" by stem — same app.
        let c = capture(&[window(1, "code"), window(2, "Code.exe")], &apps);
        assert_eq!(c.steps.len(), 1);
    }

    #[test]
    fn summary_reads_naturally() {
        let c = capture(&[window(1, "Slack")], &installed());
        assert_eq!(summary(&c), "1 app: Slack");
        let c = capture(&[window(1, "Slack"), window(2, "Code")], &installed());
        assert_eq!(summary(&c), "2 apps: Slack, Visual Studio Code");
        let c = capture(&[], &installed());
        assert_eq!(summary(&c), "No open apps match an installed app");
    }
}
