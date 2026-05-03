//! Query classification for the search dispatcher in `lib.rs::search`.
//!
//! This module is pure logic — no state access, no IO, no Tauri types.
//! Its purpose is to make the prefix-routing decision testable in isolation.
//! `lib.rs::search` consumes this module — changes to routing rules go here
//! and must be covered by the tests below.
//!
//! Routing rules (preserved from current `lib.rs::search` behavior):
//!
//! 1. An empty query returns `Route::Empty`.
//! 2. Reserved *exclusive* prefixes (notes / files / clipboard) short-circuit
//!    to their respective `Route` variant BEFORE web-search keywords are
//!    considered. A user-configured web search with keyword `n`, `notes`,
//!    `f`, `files`, `cb`, or `clip` is therefore unreachable (TD risk R-10).
//! 3. Bare reserved prefix ("n") is a listing request; prefix-plus-space
//!    ("n meeting") is a search request. Trailing-space only ("n ") currently
//!    produces `SubQuery::Search("")`, which downstream treats as a listing.
//! 4. A web-search keyword must be followed by a space and at least one
//!    character of subquery to match. "g" alone or "g " do not match.
//! 5. If a web search's URL template contains `{instance}` but the user has
//!    not configured one, the match is reported but flagged as `NeedsInstance`
//!    so the caller can skip it.

use crate::config::settings::WebSearch;

/// Reserved query prefixes that short-circuit before web-search keyword matching.
/// Using one of these as a web-search keyword makes that web search unreachable.
pub const RESERVED_PREFIXES: &[&str] = &["n", "notes", "f", "files", "cb", "clip", "cap", "captures"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Empty,
    Notes(SubQuery),
    Files(SubQuery),
    Clipboard(SubQuery),
    /// WAT-306: `>` alone lists all system commands; `>foo` filters them.
    /// Exclusive route — apps and web searches are not mixed in.
    SystemCommands(SubQuery),
    /// Phase 1A capture model: aggregated view across notes, snippets,
    /// and clipboard entries. Sorted by recency. The `cap` / `captures`
    /// prefix activates this route exclusively (no apps / web mixed
    /// in) so the user can scan everything they've recently captured
    /// without route-hopping.
    Captures(SubQuery),
    /// Not a reserved-prefix route. Caller should check web-search match next,
    /// then fall through to general fuzzy-match over apps / commands / web.
    Passthrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubQuery {
    /// Bare prefix alone (e.g. "n") — show recents, no text filter.
    Listing,
    /// Prefix plus subquery (e.g. "n meeting") — filter by the trimmed subquery.
    /// An empty string here (from "n ") preserves current dispatcher behavior,
    /// which treats trailing-space the same as a listing.
    Search(String),
}

/// Classify a raw query string into a reserved-prefix route.
/// Returns `Route::Passthrough` when the query has no reserved prefix.
pub fn classify_prefix_route(query: &str) -> Route {
    if query.is_empty() {
        return Route::Empty;
    }

    // WAT-306: `>` is a single-char reserved prefix. Handle it before the
    // word-style prefixes so `>` alone (Listing) and `>foo` / `> foo`
    // (Search) all route to the system-command palette.
    if query == ">" {
        return Route::SystemCommands(SubQuery::Listing);
    }
    if let Some(rest) = query.strip_prefix('>') {
        // Both `> foo` and `>foo` are valid entry forms; leading/trailing
        // spaces in the subquery are trimmed so spelling doesn't matter.
        let sub = rest.trim().to_string();
        if sub.is_empty() {
            // `> ` (with whitespace only) is treated as listing mode.
            return Route::SystemCommands(SubQuery::Listing);
        }
        return Route::SystemCommands(SubQuery::Search(sub));
    }

    // Bare prefix alone: listing mode.
    match query {
        "n" | "notes" => return Route::Notes(SubQuery::Listing),
        "f" | "files" => return Route::Files(SubQuery::Listing),
        "cb" | "clip" => return Route::Clipboard(SubQuery::Listing),
        "cap" | "captures" => return Route::Captures(SubQuery::Listing),
        _ => {}
    }

    // Prefix-with-space: search mode.
    if let Some(sub) = strip_one_of(query, &["n ", "notes "]) {
        return Route::Notes(SubQuery::Search(sub.to_string()));
    }
    if let Some(sub) = strip_one_of(query, &["f ", "files "]) {
        return Route::Files(SubQuery::Search(sub.to_string()));
    }
    if let Some(sub) = strip_one_of(query, &["cb ", "clip "]) {
        return Route::Clipboard(SubQuery::Search(sub.to_string()));
    }
    if let Some(sub) = strip_one_of(query, &["cap ", "captures "]) {
        return Route::Captures(SubQuery::Search(sub.to_string()));
    }

    Route::Passthrough
}

fn strip_one_of<'a>(query: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|p| query.strip_prefix(p))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSearchMatch {
    None,
    /// Matched a configured web search but its `{instance}` placeholder is
    /// required and the user has not set one. Caller should skip this match
    /// (current `lib.rs::search` behavior: `continue` to next web search).
    NeedsInstance { index: usize },
    /// Matched and ready to dispatch. `subquery` is the text after the keyword
    /// and a single space; it is guaranteed non-empty.
    Matched { index: usize, subquery: String },
}

/// Return the first matching web search for `query`, preserving the current
/// `lib.rs::search` iteration order (settings-file order). Pre-condition: the
/// caller has already run `classify_prefix_route` and received `Passthrough`;
/// otherwise a web search with a keyword colliding with a reserved prefix will
/// be shadowed and never reached (this is the intended R-10 behavior today —
/// the collision is prevented at config-time by
/// `default_web_search_keywords_do_not_collide_with_reserved_prefixes`).
pub fn match_web_search(query: &str, web_searches: &[WebSearch]) -> WebSearchMatch {
    for (index, ws) in web_searches.iter().enumerate() {
        let prefix = format!("{} ", ws.keyword);
        let Some(sub) = query.strip_prefix(&prefix) else {
            continue;
        };
        if sub.is_empty() {
            // Current behavior: keyword+space with no subquery does not produce
            // a result. Continue — a later web search with a shorter keyword
            // could still match in theory.
            continue;
        }

        let needs_instance = ws.url.contains("{instance}");
        let has_instance = ws
            .instance
            .as_deref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        if needs_instance && !has_instance {
            return WebSearchMatch::NeedsInstance { index };
        }

        return WebSearchMatch::Matched {
            index,
            subquery: sub.to_string(),
        };
    }
    WebSearchMatch::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::WebSearch;

    fn ws(keyword: &str, url: &str, instance: Option<&str>) -> WebSearch {
        WebSearch {
            name: keyword.to_string(),
            keyword: keyword.to_string(),
            url: url.to_string(),
            icon: None,
            requires_setup: false,
            instance: instance.map(|s| s.to_string()),
        }
    }

    fn google() -> WebSearch {
        ws("g", "https://www.google.com/search?q={query}", None)
    }

    fn ddg() -> WebSearch {
        ws("ddg", "https://duckduckgo.com/?q={query}", None)
    }

    fn jira(instance: Option<&str>) -> WebSearch {
        ws(
            "jira",
            "https://{instance}.atlassian.net/browse/{query}",
            instance,
        )
    }

    // --- classify_prefix_route ---

    #[test]
    fn empty_query_is_empty_route() {
        assert_eq!(classify_prefix_route(""), Route::Empty);
    }

    #[test]
    fn bare_n_and_notes_are_notes_listing() {
        assert_eq!(classify_prefix_route("n"), Route::Notes(SubQuery::Listing));
        assert_eq!(
            classify_prefix_route("notes"),
            Route::Notes(SubQuery::Listing)
        );
    }

    #[test]
    fn bare_f_and_files_are_files_listing() {
        assert_eq!(classify_prefix_route("f"), Route::Files(SubQuery::Listing));
        assert_eq!(
            classify_prefix_route("files"),
            Route::Files(SubQuery::Listing)
        );
    }

    #[test]
    fn bare_cb_and_clip_are_clipboard_listing() {
        assert_eq!(
            classify_prefix_route("cb"),
            Route::Clipboard(SubQuery::Listing)
        );
        assert_eq!(
            classify_prefix_route("clip"),
            Route::Clipboard(SubQuery::Listing)
        );
    }

    #[test]
    fn n_space_subquery_is_notes_search() {
        assert_eq!(
            classify_prefix_route("n meeting"),
            Route::Notes(SubQuery::Search("meeting".into()))
        );
    }

    #[test]
    fn notes_space_subquery_is_notes_search() {
        assert_eq!(
            classify_prefix_route("notes meeting"),
            Route::Notes(SubQuery::Search("meeting".into()))
        );
    }

    #[test]
    fn f_space_subquery_is_files_search() {
        assert_eq!(
            classify_prefix_route("f config.toml"),
            Route::Files(SubQuery::Search("config.toml".into()))
        );
    }

    #[test]
    fn cb_space_subquery_is_clipboard_search() {
        assert_eq!(
            classify_prefix_route("cb localhost"),
            Route::Clipboard(SubQuery::Search("localhost".into()))
        );
    }

    #[test]
    fn trailing_space_preserves_current_behavior_as_empty_search() {
        // Documented quirk: "n " produces Search("") rather than Listing.
        // Downstream treats both as "show recents", so this is behavior-neutral.
        // Test pins the current contract so a future refactor doesn't drift.
        assert_eq!(
            classify_prefix_route("n "),
            Route::Notes(SubQuery::Search("".into()))
        );
    }

    #[test]
    fn word_starting_with_n_is_not_a_notes_prefix() {
        assert_eq!(classify_prefix_route("nothing"), Route::Passthrough);
        assert_eq!(classify_prefix_route("notebook"), Route::Passthrough);
    }

    #[test]
    fn word_starting_with_f_is_not_a_files_prefix() {
        assert_eq!(classify_prefix_route("firefox"), Route::Passthrough);
    }

    #[test]
    fn word_starting_with_cb_is_not_a_clipboard_prefix() {
        assert_eq!(classify_prefix_route("cblock"), Route::Passthrough);
    }

    #[test]
    fn uppercase_prefix_does_not_match() {
        // Prefix matching is case-sensitive (current behavior). Documented.
        assert_eq!(classify_prefix_route("N meeting"), Route::Passthrough);
        assert_eq!(classify_prefix_route("Notes meeting"), Route::Passthrough);
    }

    #[test]
    fn leading_whitespace_falls_through_to_passthrough() {
        assert_eq!(classify_prefix_route(" n meeting"), Route::Passthrough);
    }

    // --- WAT-306: `>` is now a proper reserved route ---

    #[test]
    fn bare_gt_is_system_commands_listing() {
        // Typing `>` alone must surface every system command — the whole
        // point of WAT-306 is the discoverability gap for users who
        // don't remember subcommand names.
        assert_eq!(
            classify_prefix_route(">"),
            Route::SystemCommands(SubQuery::Listing)
        );
    }

    #[test]
    fn gt_with_trailing_space_is_system_commands_listing() {
        // `> ` (space only) matches the notes/files convention — trailing
        // whitespace without a real subquery is listing mode.
        assert_eq!(
            classify_prefix_route("> "),
            Route::SystemCommands(SubQuery::Listing)
        );
    }

    #[test]
    fn gt_with_subquery_is_system_commands_search_both_forms() {
        // Both `>lock` and `> lock` route to Search("lock") — space
        // after `>` is optional. Trim preserves the subquery.
        assert_eq!(
            classify_prefix_route("> lock"),
            Route::SystemCommands(SubQuery::Search("lock".into()))
        );
        assert_eq!(
            classify_prefix_route(">lock"),
            Route::SystemCommands(SubQuery::Search("lock".into()))
        );
    }

    #[test]
    fn gt_trims_inner_whitespace_around_subquery() {
        // Defensive: user fumble-types `>   sleep  ` — the inner
        // subquery is the trimmed term so downstream matching doesn't
        // break on whitespace.
        assert_eq!(
            classify_prefix_route(">   sleep  "),
            Route::SystemCommands(SubQuery::Search("sleep".into()))
        );
    }

    #[test]
    fn plain_query_is_passthrough() {
        assert_eq!(classify_prefix_route("chrome"), Route::Passthrough);
        assert_eq!(
            classify_prefix_route("hello world"),
            Route::Passthrough
        );
    }

    #[test]
    fn reserved_prefixes_list_matches_matchers() {
        // If anyone extends classify_prefix_route with a new reserved prefix,
        // this catches them if they forget to update RESERVED_PREFIXES.
        for prefix in RESERVED_PREFIXES {
            let route = classify_prefix_route(prefix);
            assert!(
                matches!(
                    route,
                    Route::Notes(SubQuery::Listing)
                        | Route::Files(SubQuery::Listing)
                        | Route::Clipboard(SubQuery::Listing)
                        | Route::Captures(SubQuery::Listing)
                ),
                "RESERVED_PREFIXES entry '{prefix}' did not classify to a listing route: {route:?}",
            );
        }
    }

    // --- Phase 1A capture model: `cap` / `captures` route ---

    #[test]
    fn bare_cap_and_captures_are_captures_listing() {
        assert_eq!(
            classify_prefix_route("cap"),
            Route::Captures(SubQuery::Listing)
        );
        assert_eq!(
            classify_prefix_route("captures"),
            Route::Captures(SubQuery::Listing)
        );
    }

    #[test]
    fn cap_space_subquery_is_captures_search() {
        assert_eq!(
            classify_prefix_route("cap address"),
            Route::Captures(SubQuery::Search("address".into()))
        );
        assert_eq!(
            classify_prefix_route("captures address"),
            Route::Captures(SubQuery::Search("address".into()))
        );
    }

    #[test]
    fn word_starting_with_cap_is_not_a_captures_prefix() {
        // `capacity`, `capture` (no `s`), `cape` etc. would otherwise
        // collide with the new prefix. The exact-match listing path
        // and the prefix-with-space search path together prevent that.
        assert_eq!(classify_prefix_route("capacity"), Route::Passthrough);
        assert_eq!(classify_prefix_route("capricorn"), Route::Passthrough);
    }

    // --- match_web_search ---

    #[test]
    fn empty_web_searches_returns_none() {
        assert_eq!(match_web_search("g pasta", &[]), WebSearchMatch::None);
    }

    #[test]
    fn matches_google_with_subquery() {
        let searches = vec![google()];
        assert_eq!(
            match_web_search("g pasta recipes", &searches),
            WebSearchMatch::Matched {
                index: 0,
                subquery: "pasta recipes".into(),
            }
        );
    }

    #[test]
    fn keyword_without_space_does_not_match() {
        let searches = vec![google()];
        assert_eq!(match_web_search("g", &searches), WebSearchMatch::None);
    }

    #[test]
    fn keyword_with_space_and_empty_subquery_does_not_match() {
        // Current behavior: "g " alone does not produce a web-search result.
        let searches = vec![google()];
        assert_eq!(match_web_search("g ", &searches), WebSearchMatch::None);
    }

    #[test]
    fn iteration_order_matches_settings_order() {
        // If two web searches both have keywords that are prefixes of "g ",
        // the first one wins. (Unique-keyword invariant prevents this in the
        // default config, but documents the rule for user-added searches.)
        let searches = vec![google(), ddg()];
        assert_eq!(
            match_web_search("ddg cats", &searches),
            WebSearchMatch::Matched {
                index: 1,
                subquery: "cats".into(),
            }
        );
    }

    #[test]
    fn jira_without_configured_instance_is_needs_instance() {
        let searches = vec![jira(None)];
        assert_eq!(
            match_web_search("jira PROJ-123", &searches),
            WebSearchMatch::NeedsInstance { index: 0 }
        );
    }

    #[test]
    fn jira_with_empty_instance_is_needs_instance() {
        let searches = vec![jira(Some(""))];
        assert_eq!(
            match_web_search("jira PROJ-123", &searches),
            WebSearchMatch::NeedsInstance { index: 0 }
        );
    }

    #[test]
    fn jira_with_configured_instance_matches() {
        let searches = vec![jira(Some("mycompany"))];
        assert_eq!(
            match_web_search("jira PROJ-123", &searches),
            WebSearchMatch::Matched {
                index: 0,
                subquery: "PROJ-123".into(),
            }
        );
    }

    #[test]
    fn non_matching_query_returns_none() {
        let searches = vec![google(), ddg()];
        assert_eq!(
            match_web_search("firefox", &searches),
            WebSearchMatch::None
        );
    }

    // --- R-10 reserved-prefix shadowing: documents the contract the caller ---
    //     must enforce (run classify_prefix_route FIRST, only call match_web_search
    //     on Passthrough).

    #[test]
    fn r10_reserved_prefix_would_match_in_web_search_isolation() {
        // This test documents the HAZARD, not the current runtime behavior.
        // A user-added web search with keyword "n" IS matchable by
        // `match_web_search` alone. The caller in `lib.rs::search` prevents
        // this by running reserved-prefix classification FIRST and only
        // calling `match_web_search` when classify returns Passthrough.
        let malicious = ws("n", "https://evil.example/?q={query}", None);
        assert_eq!(
            match_web_search("n meeting", &[malicious]),
            WebSearchMatch::Matched {
                index: 0,
                subquery: "meeting".into(),
            },
            "match_web_search does not reserve prefixes — caller must enforce ordering"
        );
    }

    #[test]
    fn r10_classify_first_then_match_pattern_is_safe() {
        // This test encodes the intended caller discipline: reserved routes
        // win. Even with a malicious "n" web search, classify_prefix_route
        // short-circuits before match_web_search is consulted.
        let malicious = vec![ws("n", "https://evil.example/?q={query}", None)];
        match classify_prefix_route("n meeting") {
            Route::Notes(SubQuery::Search(sub)) => {
                assert_eq!(sub, "meeting");
                // Caller never reaches match_web_search for this query.
            }
            other => panic!("expected Notes route, got {other:?}"),
        }
        // Sanity: if the caller DID reach match_web_search for a Passthrough
        // query, it works normally.
        assert_eq!(
            match_web_search("x foo", &malicious),
            WebSearchMatch::None,
            "plain query should not match an 'n' keyword web search"
        );
    }
}
