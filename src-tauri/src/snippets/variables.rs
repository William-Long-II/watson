//! Template variables inside snippet expansions.
//!
//! Expanded at paste time (see `actions::handlers::paste_snippet`):
//!
//! - `{clipboard}` — the current clipboard text (empty if the
//!   clipboard holds no text).
//! - `{date}` / `{date:FORMAT}` — today's local date. Default format
//!   `%Y-%m-%d`; `FORMAT` is any chrono strftime string.
//! - `{time}` / `{time:FORMAT}` — the current local time. Default
//!   format `%H:%M`.
//! - `{input:Prompt}` — a value the user types in the launcher before
//!   the paste happens. The same prompt used twice is asked once and
//!   substituted in both places.
//!
//! Anything else in braces — an unknown name, an invalid strftime
//! format, an empty input prompt — is left verbatim, so snippets that
//! already contain literal braces (code, JSON) keep pasting as-is.

use chrono::format::{Item, StrftimeItems};
use chrono::{DateTime, TimeZone};
use std::collections::HashMap;

const DEFAULT_DATE_FORMAT: &str = "%Y-%m-%d";
const DEFAULT_TIME_FORMAT: &str = "%H:%M";

/// Values available to the expander. Built by the caller so the
/// expansion itself stays pure and testable.
pub struct Context<'a, Tz: TimeZone> {
    pub clipboard: Option<&'a str>,
    pub now: DateTime<Tz>,
    pub inputs: &'a HashMap<String, String>,
}

enum Token<'a> {
    Clipboard,
    Date(&'a str),
    Time(&'a str),
    Input(&'a str),
}

fn parse_token(inner: &str) -> Option<Token<'_>> {
    let (name, arg) = match inner.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (inner, None),
    };
    match (name, arg) {
        ("clipboard", None) => Some(Token::Clipboard),
        ("date", None) => Some(Token::Date(DEFAULT_DATE_FORMAT)),
        ("date", Some(f)) if valid_format(f) => Some(Token::Date(f)),
        ("time", None) => Some(Token::Time(DEFAULT_TIME_FORMAT)),
        ("time", Some(f)) if valid_format(f) => Some(Token::Time(f)),
        ("input", Some(p)) if !p.trim().is_empty() => Some(Token::Input(p.trim())),
        _ => None,
    }
}

/// chrono's `Display` for a bad format string errors out (and
/// `to_string` panics), so reject those up front.
fn valid_format(fmt: &str) -> bool {
    !fmt.is_empty() && StrftimeItems::new(fmt).all(|item| !matches!(item, Item::Error))
}

enum Piece<'a> {
    Text(&'a str),
    Var(Token<'a>),
}

/// Split `template` into literal runs and recognised `{...}`
/// variables. Unrecognised braces stay in the literal text.
fn scan(template: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let inner = &after[..close];
        // `{ {date}` — restart at the inner brace so the real
        // variable still expands.
        if let Some(nested) = inner.rfind('{') {
            pieces.push(Piece::Text(&rest[..open + 1 + nested]));
            rest = &rest[open + 1 + nested..];
            continue;
        }
        match parse_token(inner) {
            Some(token) => {
                pieces.push(Piece::Text(&rest[..open]));
                pieces.push(Piece::Var(token));
            }
            None => pieces.push(Piece::Text(&rest[..open + 1 + close + 1])),
        }
        rest = &after[close + 1..];
    }
    pieces.push(Piece::Text(rest));
    pieces
}

/// Distinct `{input:Prompt}` prompts in the order they first appear.
/// The launcher asks for these before pasting.
pub fn input_prompts(template: &str) -> Vec<String> {
    let mut prompts: Vec<String> = Vec::new();
    for piece in scan(template) {
        if let Piece::Var(Token::Input(p)) = piece {
            if !prompts.iter().any(|existing| existing == p) {
                prompts.push(p.to_string());
            }
        }
    }
    prompts
}

/// Substitute every variable in `template`. A missing input value
/// expands to the empty string.
pub fn expand<Tz: TimeZone>(template: &str, ctx: &Context<'_, Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = String::with_capacity(template.len());
    for piece in scan(template) {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Var(Token::Clipboard) => out.push_str(ctx.clipboard.unwrap_or("")),
            Piece::Var(Token::Date(f) | Token::Time(f)) => {
                out.push_str(&ctx.now.format(f).to_string())
            }
            Piece::Var(Token::Input(p)) => {
                out.push_str(ctx.inputs.get(p).map(String::as_str).unwrap_or(""))
            }
        }
    }
    out
}

/// Cheap check so the paste path can skip reading the clipboard when
/// the snippet doesn't need it.
pub fn uses_clipboard(template: &str) -> bool {
    scan(template)
        .iter()
        .any(|piece| matches!(piece, Piece::Var(Token::Clipboard)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 7, 9, 5, 30).unwrap()
    }

    fn expand_with(template: &str, clipboard: Option<&str>, inputs: &[(&str, &str)]) -> String {
        let inputs: HashMap<String, String> = inputs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        expand(
            template,
            &Context {
                clipboard,
                now: now(),
                inputs: &inputs,
            },
        )
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(expand_with("hello\nworld", None, &[]), "hello\nworld");
    }

    #[test]
    fn clipboard_expands_to_clipboard_text() {
        assert_eq!(
            expand_with("see {clipboard}!", Some("abc"), &[]),
            "see abc!"
        );
    }

    #[test]
    fn clipboard_without_text_expands_to_empty() {
        assert_eq!(expand_with("[{clipboard}]", None, &[]), "[]");
    }

    #[test]
    fn date_and_time_use_default_formats() {
        assert_eq!(expand_with("{date} {time}", None, &[]), "2026-03-07 09:05");
    }

    #[test]
    fn date_and_time_accept_custom_formats() {
        assert_eq!(
            expand_with("{date:%d/%m/%Y} at {time:%H:%M:%S}", None, &[]),
            "07/03/2026 at 09:05:30"
        );
        assert_eq!(
            expand_with("{date:%A, %B %-d}", None, &[]),
            "Saturday, March 7"
        );
    }

    #[test]
    fn date_uses_the_supplied_timezone() {
        let tz = FixedOffset::east_opt(-10 * 3600).unwrap();
        let inputs = HashMap::new();
        let ctx = Context {
            clipboard: None,
            now: now().with_timezone(&tz),
            inputs: &inputs,
        };
        assert_eq!(expand("{date} {time}", &ctx), "2026-03-06 23:05");
    }

    #[test]
    fn invalid_format_is_left_verbatim() {
        assert_eq!(expand_with("{date:%Q}", None, &[]), "{date:%Q}");
        assert_eq!(expand_with("{time:}", None, &[]), "{time:}");
    }

    #[test]
    fn input_expands_to_supplied_value_everywhere() {
        assert_eq!(
            expand_with(
                "Hi {input:Name}, bye {input:Name}",
                None,
                &[("Name", "Ada")]
            ),
            "Hi Ada, bye Ada"
        );
    }

    #[test]
    fn missing_input_expands_to_empty() {
        assert_eq!(expand_with("Hi {input:Name}.", None, &[]), "Hi .");
    }

    #[test]
    fn input_values_are_not_re_expanded() {
        assert_eq!(
            expand_with("{input:X}", Some("clip"), &[("X", "{clipboard}")]),
            "{clipboard}"
        );
    }

    #[test]
    fn unknown_and_literal_braces_are_preserved() {
        assert_eq!(
            expand_with("{foo} {} {input} {input: }", None, &[]),
            "{foo} {} {input} {input: }"
        );
        assert_eq!(
            expand_with("fn main() { println!(\"{}\", x); }", None, &[]),
            "fn main() { println!(\"{}\", x); }"
        );
        assert_eq!(expand_with("{\"a\": 1}", None, &[]), "{\"a\": 1}");
    }

    #[test]
    fn unclosed_brace_is_preserved() {
        assert_eq!(expand_with("a {date", None, &[]), "a {date");
    }

    #[test]
    fn nested_brace_still_expands_inner_variable() {
        assert_eq!(expand_with("{ {date}", None, &[]), "{ 2026-03-07");
        assert_eq!(expand_with("{{clipboard}}", Some("x"), &[]), "{x}");
    }

    #[test]
    fn multibyte_text_is_preserved() {
        assert_eq!(
            expand_with("日付: {date} ✓", None, &[]),
            "日付: 2026-03-07 ✓"
        );
    }

    #[test]
    fn input_prompts_are_distinct_and_ordered() {
        assert_eq!(
            input_prompts("{input:Name} {date} {input:Company} {input:Name} {input:}"),
            vec!["Name".to_string(), "Company".to_string()]
        );
        assert!(input_prompts("no vars {clipboard}").is_empty());
    }

    #[test]
    fn input_prompt_is_trimmed() {
        assert_eq!(
            input_prompts("{input: Ticket id }"),
            vec!["Ticket id".to_string()]
        );
        assert_eq!(
            expand_with("{input: Ticket id }", None, &[("Ticket id", "42")]),
            "42"
        );
    }

    #[test]
    fn uses_clipboard_detects_variable() {
        assert!(uses_clipboard("x {clipboard} y"));
        assert!(!uses_clipboard("x {date} y"));
    }
}
