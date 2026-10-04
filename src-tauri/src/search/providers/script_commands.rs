//! WAT-501 / WAT-502: user-defined script command provider.
//!
//! A script command pairs a keyword with a script on disk. When the
//! query is `<keyword> <text>`, the dispatcher in `lib.rs::search`
//! routes here exclusively: the script runs with `<text>` as its first
//! argument (also exported as `WATSON_QUERY`), and its stdout is parsed
//! as the versioned JSON contract below and turned into result rows.
//!
//! ```json
//! { "version": 1,
//!   "items": [ { "title": "…", "subtitle": "…", "icon": "🌤",
//!                "action": { "type": "open_url", "url": "https://…" } } ] }
//! ```
//!
//! A bare top-level array of items is accepted as shorthand for
//! version 1. Unknown fields are ignored so later versions can add
//! fields without breaking older Watson builds. The full contract is
//! documented in the README ("Script commands").
//!
//! Sandboxing is OS-level only: the script runs as the current user
//! with the user's permissions. The guard rails here are a timeout, an
//! stdout size cap, and a closed set of result actions (no arbitrary
//! command execution from a script's output).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::settings::ScriptCommand;
use crate::search::provider::ResultProvider;
use crate::search::{ResultType, SearchAction, SearchResult};

/// Highest stdout contract version this build understands.
pub const SCRIPT_OUTPUT_VERSION: u32 = 1;

/// Scripts emitting more than this many bytes on stdout are killed and
/// reported as an error rather than parsed.
const MAX_STDOUT_BYTES: u64 = 1024 * 1024;

/// Only the head of stderr is kept for the error row.
const MAX_STDERR_BYTES: u64 = 16 * 1024;

/// Rows beyond this are dropped; the launcher only shows a handful.
const MAX_ITEMS: usize = 50;

/// Same tier as web-search rows. Each row gets `BASE - index` so the
/// script's own ordering is preserved if anything downstream sorts.
const SCRIPT_RESULT_SCORE: i64 = 10_000;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ScriptOutput {
    Items(Vec<ScriptItem>),
    Envelope {
        #[serde(default = "default_version")]
        version: u32,
        #[serde(default)]
        items: Vec<ScriptItem>,
    },
}

fn default_version() -> u32 {
    SCRIPT_OUTPUT_VERSION
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ScriptItem {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
    #[serde(default)]
    pub action: Option<ScriptAction>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScriptAction {
    OpenUrl { url: String },
    CopyClipboard { content: String },
    OpenFile { path: String },
}

/// Parse a script's stdout into items. Errors carry a message meant to
/// be shown to the script's author.
pub fn parse_script_output(stdout: &str) -> Result<Vec<ScriptItem>, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let parsed: ScriptOutput = serde_json::from_str(trimmed).map_err(|e| {
        format!("stdout is not valid script JSON ({e}). Expected {{\"items\": [{{\"title\": …}}]}}")
    })?;
    let mut items = match parsed {
        ScriptOutput::Items(items) => items,
        ScriptOutput::Envelope { version, items } => {
            if version > SCRIPT_OUTPUT_VERSION {
                return Err(format!(
                    "script output version {version} is newer than this Watson supports ({SCRIPT_OUTPUT_VERSION})"
                ));
            }
            items
        }
    };
    items.truncate(MAX_ITEMS);
    Ok(items)
}

fn to_search_action(item: &ScriptItem) -> Result<SearchAction, String> {
    match &item.action {
        None => Ok(SearchAction::CopyClipboard {
            content: item.title.clone(),
        }),
        Some(ScriptAction::OpenUrl { url }) => {
            let lower = url.trim_start().to_ascii_lowercase();
            if ["http://", "https://", "mailto:"].iter().any(|p| lower.starts_with(p)) {
                Ok(SearchAction::OpenUrl { url: url.clone() })
            } else {
                Err(format!("open_url only accepts http, https or mailto URLs (got \"{url}\")"))
            }
        }
        Some(ScriptAction::CopyClipboard { content }) => Ok(SearchAction::CopyClipboard {
            content: content.clone(),
        }),
        Some(ScriptAction::OpenFile { path }) => Ok(SearchAction::OpenFile {
            path: expand_tilde(path).to_string_lossy().into_owned(),
        }),
    }
}

/// Convert parsed items into result rows. Items with a disallowed
/// action are replaced by an error row so the author sees why.
pub fn items_to_results(cmd: &ScriptCommand, items: Vec<ScriptItem>) -> Vec<SearchResult> {
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match to_search_action(&item) {
            Ok(action) => SearchResult {
                id: format!(
                    "script:{}:{}",
                    cmd.keyword,
                    item.id.clone().unwrap_or_else(|| i.to_string())
                ),
                name: item.title,
                description: item.subtitle.unwrap_or_else(|| cmd.name.clone()),
                icon: item.icon.or_else(|| cmd.icon.clone()),
                result_type: ResultType::ScriptCommand,
                score: SCRIPT_RESULT_SCORE - i as i64,
                frecency_score: 0.0,
                preview: item.preview,
                pinned: false,
                action,
            },
            Err(msg) => error_result(cmd, &msg, i),
        })
        .collect()
}

/// A single row explaining why the script produced no usable output.
/// Selecting it copies the full message so it can be pasted elsewhere.
fn error_result(cmd: &ScriptCommand, message: &str, index: usize) -> SearchResult {
    let first_line = message.lines().next().unwrap_or("").trim();
    let description: String = first_line.chars().take(200).collect();
    SearchResult {
        id: format!("script:{}:error:{index}", cmd.keyword),
        name: format!("{}: script error", cmd.name),
        description,
        icon: Some("⚠️".to_string()),
        result_type: ResultType::ScriptCommand,
        score: SCRIPT_RESULT_SCORE - index as i64,
        frecency_score: 0.0,
        preview: None,
        pinned: false,
        action: SearchAction::CopyClipboard {
            content: message.to_string(),
        },
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("~/").or_else(|| trimmed.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

/// Pick the program + leading args used to run `script`. An explicit
/// interpreter wins; otherwise the extension decides; otherwise the
/// script is executed directly.
pub fn build_invocation(script: &Path, interpreter: Option<&str>) -> (String, Vec<String>) {
    let script_arg = script.to_string_lossy().into_owned();
    if let Some(interp) = interpreter.map(str::trim).filter(|s| !s.is_empty()) {
        let mut parts = interp.split_whitespace().map(str::to_string);
        let program = parts.next().unwrap_or_default();
        let mut args: Vec<String> = parts.collect();
        args.push(script_arg);
        return (program, args);
    }
    let ext = script
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let python = if cfg!(windows) { "python" } else { "python3" };
    let (program, mut args): (&str, Vec<String>) = match ext.as_str() {
        "py" => (python, vec![]),
        "js" | "mjs" | "cjs" => ("node", vec![]),
        "sh" => ("sh", vec![]),
        "rb" => ("ruby", vec![]),
        "ps1" => (
            "powershell",
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
            ],
        ),
        _ => return (script_arg, vec![]),
    };
    args.push(script_arg);
    (program.to_string(), args)
}

/// Run the script with `query` and return its stdout. Non-zero exit,
/// timeout, oversized output, and spawn failures are all errors.
pub async fn run_script(cmd: &ScriptCommand, query: &str) -> Result<String, String> {
    let script = expand_tilde(&cmd.script);
    let (program, mut args) = build_invocation(&script, cmd.interpreter.as_deref());
    args.push(query.to_string());

    let mut command = Command::new(&program);
    command
        .args(&args)
        .env("WATSON_QUERY", query)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = script.parent().filter(|d| d.is_dir()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: don't flash a console for every keystroke.
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", program))?;
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let run = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (out_res, err_res) = tokio::join!(
            async { (&mut stdout).take(MAX_STDOUT_BYTES + 1).read_to_end(&mut out).await },
            async {
                // Keep the head for the error row, then drain the rest
                // so a chatty script can't block on a full stderr pipe.
                let r = (&mut stderr).take(MAX_STDERR_BYTES).read_to_end(&mut err).await;
                let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
                r
            },
        );
        out_res.map_err(|e| format!("reading stdout failed: {e}"))?;
        let _ = err_res;
        if out.len() as u64 > MAX_STDOUT_BYTES {
            return Err(format!("stdout exceeded {} KB", MAX_STDOUT_BYTES / 1024));
        }
        let status = child
            .wait()
            .await
            .map_err(|e| format!("waiting for script failed: {e}"))?;
        if !status.success() {
            let stderr_text = String::from_utf8_lossy(&err).trim().to_string();
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".into());
            return Err(if stderr_text.is_empty() {
                format!("script exited with status {code}")
            } else {
                format!("script exited with status {code}: {stderr_text}")
            });
        }
        String::from_utf8(out).map_err(|_| "stdout is not valid UTF-8".to_string())
    };

    // On timeout the future (and with it `child`) is dropped, and
    // `kill_on_drop` terminates the process.
    match tokio::time::timeout(Duration::from_millis(cmd.timeout_ms.max(1)), run).await {
        Ok(result) => result,
        Err(_) => Err(format!("script timed out after {} ms", cmd.timeout_ms)),
    }
}

pub struct ScriptCommandProvider<'a> {
    pub command: &'a ScriptCommand,
    /// Text after the keyword, already trimmed by the router.
    pub subquery: String,
}

#[async_trait::async_trait]
impl<'a> ResultProvider for ScriptCommandProvider<'a> {
    fn name(&self) -> &'static str {
        "script_command"
    }

    async fn search(&self, _query: &str) -> Vec<SearchResult> {
        let outcome = match run_script(self.command, &self.subquery).await {
            Ok(stdout) => parse_script_output(&stdout),
            Err(e) => Err(e),
        };
        match outcome {
            Ok(items) => items_to_results(self.command, items),
            Err(msg) => vec![error_result(self.command, &msg, 0)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(script: &str) -> ScriptCommand {
        ScriptCommand {
            name: "Test".into(),
            keyword: "t".into(),
            script: script.into(),
            interpreter: None,
            icon: Some("🧪".into()),
            timeout_ms: 5_000,
        }
    }

    // --- parse_script_output ---

    #[test]
    fn parses_envelope_with_items() {
        let items = parse_script_output(
            r#"{"version":1,"items":[{"title":"A","subtitle":"a","action":{"type":"open_url","url":"https://x.test"}}]}"#,
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "A");
        assert_eq!(
            items[0].action,
            Some(ScriptAction::OpenUrl {
                url: "https://x.test".into()
            })
        );
    }

    #[test]
    fn parses_bare_array_shorthand() {
        let items = parse_script_output(r#"[{"title":"A"},{"title":"B"}]"#).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn missing_version_defaults_to_v1_and_unknown_fields_are_ignored() {
        let items =
            parse_script_output(r#"{"items":[{"title":"A","future_field":42}],"extra":true}"#)
                .unwrap();
        assert_eq!(items[0].title, "A");
    }

    #[test]
    fn empty_stdout_is_no_results() {
        assert!(parse_script_output("  \n").unwrap().is_empty());
    }

    #[test]
    fn newer_version_is_rejected() {
        let err = parse_script_output(r#"{"version":2,"items":[]}"#).unwrap_err();
        assert!(err.contains("version 2"));
    }

    #[test]
    fn item_without_title_is_rejected() {
        assert!(parse_script_output(r#"[{"subtitle":"no title"}]"#).is_err());
    }

    #[test]
    fn unknown_action_type_is_rejected() {
        assert!(parse_script_output(
            r#"[{"title":"x","action":{"type":"run_command","command":"rm -rf /"}}]"#
        )
        .is_err());
    }

    #[test]
    fn non_json_is_rejected_with_hint() {
        let err = parse_script_output("hello").unwrap_err();
        assert!(err.contains("items"));
    }

    #[test]
    fn items_are_capped() {
        let many: Vec<String> = (0..80).map(|i| format!(r#"{{"title":"{i}"}}"#)).collect();
        let items = parse_script_output(&format!("[{}]", many.join(","))).unwrap();
        assert_eq!(items.len(), MAX_ITEMS);
    }

    // --- items_to_results ---

    #[test]
    fn results_preserve_order_and_fall_back_to_command_icon_and_name() {
        let c = cmd("/x.sh");
        let items = parse_script_output(r#"[{"title":"A"},{"title":"B","icon":"🌤","subtitle":"b"}]"#)
            .unwrap();
        let rows = items_to_results(&c, items);
        assert!(rows[0].score > rows[1].score);
        assert_eq!(rows[0].description, "Test");
        assert_eq!(rows[0].icon.as_deref(), Some("🧪"));
        assert_eq!(rows[1].icon.as_deref(), Some("🌤"));
        assert!(matches!(rows[0].result_type, ResultType::ScriptCommand));
        // No action → copy the title.
        assert!(matches!(&rows[0].action, SearchAction::CopyClipboard { content } if content == "A"));
    }

    #[test]
    fn non_http_open_url_becomes_error_row() {
        let c = cmd("/x.sh");
        let items =
            parse_script_output(r#"[{"title":"A","action":{"type":"open_url","url":"javascript:alert(1)"}}]"#)
                .unwrap();
        let rows = items_to_results(&c, items);
        assert_eq!(rows[0].name, "Test: script error");
    }

    // --- build_invocation ---

    #[test]
    fn invocation_uses_extension_or_explicit_interpreter() {
        let (p, a) = build_invocation(Path::new("/s/x.js"), None);
        assert_eq!(p, "node");
        assert_eq!(a, vec!["/s/x.js"]);

        let (p, a) = build_invocation(Path::new("/s/x.js"), Some("/opt/bin/node --no-warnings"));
        assert_eq!(p, "/opt/bin/node");
        assert_eq!(a, vec!["--no-warnings", "/s/x.js"]);

        let (p, a) = build_invocation(Path::new("/s/x"), None);
        assert_eq!(p, "/s/x");
        assert!(a.is_empty());
    }

    // --- run_script (real subprocesses; unix shell only) ---

    #[cfg(unix)]
    fn write_script(dir: &tempfile::TempDir, name: &str, body: &str) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn script_receives_query_as_arg_and_env() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_script(
            &dir,
            "echo.sh",
            r#"printf '[{"title":"%s","subtitle":"%s"}]' "$1" "$WATSON_QUERY""#,
        );
        let c = cmd(&path);
        let rows = ScriptCommandProvider {
            command: &c,
            subquery: "hello world".into(),
        }
        .search("t hello world")
        .await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "hello world");
        assert_eq!(rows[0].description, "hello world");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn slow_script_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_script(&dir, "slow.sh", "sleep 5; echo '[]'");
        let mut c = cmd(&path);
        c.timeout_ms = 200;
        let started = std::time::Instant::now();
        let err = run_script(&c, "x").await.unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failing_script_surfaces_stderr_as_error_row() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_script(&dir, "fail.sh", "echo 'missing API key' >&2; exit 3");
        let c = cmd(&path);
        let rows = ScriptCommandProvider {
            command: &c,
            subquery: String::new(),
        }
        .search("t ")
        .await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Test: script error");
        assert!(rows[0].description.contains("missing API key"));
    }

    #[tokio::test]
    async fn missing_script_is_an_error_not_a_panic() {
        let c = cmd("/definitely/not/here/watson-script");
        let rows = ScriptCommandProvider {
            command: &c,
            subquery: "x".into(),
        }
        .search("t x")
        .await;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].description.contains("couldn't start"));
    }
}
