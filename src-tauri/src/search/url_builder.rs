//! Web-search URL construction with scheme and instance validation.
//!
//! This module is pure logic — no state access, no IO. It closes R-04
//! (TD risk register): user-editable web-search templates could previously
//! carry a malicious scheme like `javascript:` or redirect through an
//! attacker-controlled host via an unencoded `{instance}` placeholder.
//!
//! Rules enforced:
//!
//! 1. **Scheme allowlist.** The template must begin with `http://` or
//!    `https://`. `javascript:`, `data:`, `vbscript:`, `file:`, and any
//!    template without a scheme are rejected.
//! 2. **Instance validation.** When the template contains `{instance}`, the
//!    configured value must be a valid DNS-subdomain string: dot-separated
//!    labels, each label `[A-Za-z0-9-]` and neither starting nor ending
//!    with `-`. This rejects path/userinfo/port/query/fragment injection
//!    (`/`, `@`, `:`, `?`, `#`, `%`, space, unicode).
//! 3. **Query encoding.** The subquery is URL-encoded with the `urlencoding`
//!    crate before substitution.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlBuildError {
    /// Template scheme is not in the http/https allowlist.
    /// Carries the offending scheme prefix for diagnostics.
    InvalidScheme(String),
    /// Template does not begin with any recognizable scheme.
    MalformedTemplate,
    /// Template requires `{instance}` but none was configured (or empty).
    MissingInstance,
    /// `{instance}` value contains characters not valid for a DNS subdomain.
    InvalidInstance(String),
}

const ALLOWED_SCHEMES: &[&str] = &["https://", "http://"];

pub fn build_web_search_url(
    template: &str,
    instance: Option<&str>,
    subquery: &str,
) -> Result<String, UrlBuildError> {
    validate_scheme(template)?;

    let needs_instance = template.contains("{instance}");
    let encoded_query = urlencoding::encode(subquery);

    if needs_instance {
        let inst = instance
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(UrlBuildError::MissingInstance)?;
        validate_instance(inst)?;
        Ok(template
            .replace("{instance}", inst)
            .replace("{query}", &encoded_query))
    } else {
        Ok(template.replace("{query}", &encoded_query))
    }
}

fn validate_scheme(template: &str) -> Result<(), UrlBuildError> {
    if ALLOWED_SCHEMES.iter().any(|s| template.starts_with(s)) {
        return Ok(());
    }
    // Pull out the scheme-ish prefix for diagnostics ("javascript" from
    // "javascript:alert(1)"). If there's no ':' at all, call it malformed.
    match template.split_once(':') {
        Some((scheme, _)) if !scheme.is_empty() => {
            Err(UrlBuildError::InvalidScheme(scheme.to_string()))
        }
        _ => Err(UrlBuildError::MalformedTemplate),
    }
}

fn validate_instance(instance: &str) -> Result<(), UrlBuildError> {
    let reject = || UrlBuildError::InvalidInstance(instance.to_string());
    if instance.is_empty() {
        return Err(reject());
    }
    for label in instance.split('.') {
        if label.is_empty() {
            return Err(reject());
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(reject());
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(reject());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- scheme allowlist ---

    #[test]
    fn https_scheme_is_accepted() {
        let url =
            build_web_search_url("https://example.com/?q={query}", None, "cats").unwrap();
        assert_eq!(url, "https://example.com/?q=cats");
    }

    #[test]
    fn http_scheme_is_accepted_for_localhost_and_dev_use() {
        let url =
            build_web_search_url("http://localhost:8080/s?q={query}", None, "x").unwrap();
        assert_eq!(url, "http://localhost:8080/s?q=x");
    }

    #[test]
    fn javascript_scheme_is_rejected() {
        let err = build_web_search_url("javascript:alert(1)//{query}", None, "x").unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidScheme("javascript".into()));
    }

    #[test]
    fn data_scheme_is_rejected() {
        let err = build_web_search_url("data:text/html,<script>{query}</script>", None, "x")
            .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidScheme("data".into()));
    }

    #[test]
    fn vbscript_scheme_is_rejected() {
        let err = build_web_search_url("vbscript:msgbox(\"pwn\")", None, "x").unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidScheme("vbscript".into()));
    }

    #[test]
    fn file_scheme_is_rejected() {
        let err = build_web_search_url("file:///etc/passwd", None, "x").unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidScheme("file".into()));
    }

    #[test]
    fn template_without_scheme_is_malformed() {
        let err = build_web_search_url("example.com/?q={query}", None, "x").unwrap_err();
        // No ':' at all — malformed, not InvalidScheme.
        assert_eq!(err, UrlBuildError::MalformedTemplate);
    }

    #[test]
    fn empty_template_is_malformed() {
        let err = build_web_search_url("", None, "x").unwrap_err();
        assert_eq!(err, UrlBuildError::MalformedTemplate);
    }

    #[test]
    fn uppercase_scheme_is_rejected() {
        // Current behavior: strict match. We may relax later, but drift is worth
        // catching — lowercase schemes are canonical anyway.
        let err = build_web_search_url("HTTPS://example.com/?q={query}", None, "x").unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidScheme("HTTPS".into()));
    }

    // --- instance validation: accepted cases ---

    #[test]
    fn plain_subdomain_instance_is_accepted() {
        let url = build_web_search_url(
            "https://{instance}.atlassian.net/browse/{query}",
            Some("mycompany"),
            "PROJ-1",
        )
        .unwrap();
        assert_eq!(url, "https://mycompany.atlassian.net/browse/PROJ-1");
    }

    #[test]
    fn instance_with_dash_is_accepted() {
        let url = build_web_search_url(
            "https://{instance}.example.com/{query}",
            Some("my-company"),
            "x",
        )
        .unwrap();
        assert_eq!(url, "https://my-company.example.com/x");
    }

    #[test]
    fn instance_with_multiple_labels_is_accepted() {
        let url = build_web_search_url(
            "https://{instance}.example.com/{query}",
            Some("sub.zone"),
            "x",
        )
        .unwrap();
        assert_eq!(url, "https://sub.zone.example.com/x");
    }

    // --- instance validation: rejection cases ---

    #[test]
    fn instance_with_slash_is_rejected() {
        // Primary R-04 attack: `evil.com/` would redirect through attacker.
        let err = build_web_search_url(
            "https://{instance}.atlassian.net/browse/{query}",
            Some("evil.com/"),
            "x",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidInstance("evil.com/".into()));
    }

    #[test]
    fn instance_with_at_is_rejected() {
        let err = build_web_search_url(
            "https://{instance}.atlassian.net/browse/{query}",
            Some("user@attacker"),
            "x",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidInstance("user@attacker".into()));
    }

    #[test]
    fn instance_with_colon_is_rejected() {
        let err = build_web_search_url(
            "https://{instance}.example.com/{query}",
            Some("host:8080"),
            "x",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidInstance("host:8080".into()));
    }

    #[test]
    fn instance_with_query_or_fragment_is_rejected() {
        for bad in ["evil?x=1", "evil#y"] {
            let err = build_web_search_url(
                "https://{instance}.example.com/{query}",
                Some(bad),
                "x",
            )
            .unwrap_err();
            assert_eq!(
                err,
                UrlBuildError::InvalidInstance(bad.into()),
                "expected rejection for instance {bad:?}"
            );
        }
    }

    #[test]
    fn instance_with_percent_is_rejected() {
        // Percent-encoding games: `%2e%2e` etc.
        let err = build_web_search_url(
            "https://{instance}.example.com/{query}",
            Some("a%2eb"),
            "x",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidInstance("a%2eb".into()));
    }

    #[test]
    fn instance_with_space_is_rejected() {
        let err = build_web_search_url(
            "https://{instance}.example.com/{query}",
            Some("my company"),
            "x",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidInstance("my company".into()));
    }

    #[test]
    fn instance_with_unicode_is_rejected() {
        let err = build_web_search_url(
            "https://{instance}.example.com/{query}",
            Some("café"),
            "x",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::InvalidInstance("café".into()));
    }

    #[test]
    fn instance_label_leading_or_trailing_dash_is_rejected() {
        for bad in ["-foo", "foo-", "a.-b", "a.b-"] {
            let err = build_web_search_url(
                "https://{instance}.example.com/{query}",
                Some(bad),
                "x",
            )
            .unwrap_err();
            assert_eq!(
                err,
                UrlBuildError::InvalidInstance(bad.into()),
                "expected rejection for instance {bad:?}"
            );
        }
    }

    #[test]
    fn instance_with_empty_label_is_rejected() {
        for bad in [".foo", "foo.", "a..b"] {
            let err = build_web_search_url(
                "https://{instance}.example.com/{query}",
                Some(bad),
                "x",
            )
            .unwrap_err();
            assert_eq!(
                err,
                UrlBuildError::InvalidInstance(bad.into()),
                "expected rejection for instance {bad:?}"
            );
        }
    }

    // --- missing instance ---

    #[test]
    fn missing_instance_when_required_returns_missing_instance() {
        let err = build_web_search_url(
            "https://{instance}.atlassian.net/browse/{query}",
            None,
            "PROJ-1",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::MissingInstance);
    }

    #[test]
    fn empty_instance_when_required_returns_missing_instance() {
        let err = build_web_search_url(
            "https://{instance}.atlassian.net/browse/{query}",
            Some(""),
            "PROJ-1",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::MissingInstance);
    }

    #[test]
    fn whitespace_instance_when_required_returns_missing_instance() {
        let err = build_web_search_url(
            "https://{instance}.atlassian.net/browse/{query}",
            Some("   "),
            "PROJ-1",
        )
        .unwrap_err();
        assert_eq!(err, UrlBuildError::MissingInstance);
    }

    #[test]
    fn instance_is_ignored_when_template_has_no_placeholder() {
        // User configured an instance on a template that doesn't use one.
        // Don't fail; just ignore.
        let url = build_web_search_url(
            "https://www.google.com/search?q={query}",
            Some("anything-or-junk"),
            "cats",
        )
        .unwrap();
        assert_eq!(url, "https://www.google.com/search?q=cats");
    }

    // --- query encoding ---

    #[test]
    fn query_with_spaces_is_percent_encoded() {
        let url = build_web_search_url(
            "https://example.com/?q={query}",
            None,
            "hello world",
        )
        .unwrap();
        assert_eq!(url, "https://example.com/?q=hello%20world");
    }

    #[test]
    fn query_with_special_chars_is_percent_encoded() {
        // & = # ? are URL-reserved; must be encoded when used as query text.
        let url = build_web_search_url(
            "https://example.com/?q={query}",
            None,
            "a&b=c?d#e",
        )
        .unwrap();
        assert_eq!(url, "https://example.com/?q=a%26b%3Dc%3Fd%23e");
    }

    #[test]
    fn query_with_unicode_is_percent_encoded() {
        let url = build_web_search_url(
            "https://example.com/?q={query}",
            None,
            "café",
        )
        .unwrap();
        assert_eq!(url, "https://example.com/?q=caf%C3%A9");
    }

    #[test]
    fn empty_subquery_substitutes_as_empty_string() {
        // Current dispatch layer prevents this in practice (empty subquery is
        // filtered upstream), but the builder shouldn't panic or inject junk.
        let url =
            build_web_search_url("https://example.com/?q={query}", None, "").unwrap();
        assert_eq!(url, "https://example.com/?q=");
    }

    // --- defaults smoke ---

    #[test]
    fn all_default_templates_pass_validation() {
        // If someone edits the default web searches to something malicious or
        // ill-formed, this fires before the config hits users.
        for (keyword, template, instance) in [
            ("g", "https://www.google.com/search?q={query}", None),
            ("ddg", "https://duckduckgo.com/?q={query}", None),
            ("yt", "https://www.youtube.com/results?search_query={query}", None),
            (
                "jira",
                "https://{instance}.atlassian.net/browse/{query}",
                Some("acme"),
            ),
        ] {
            let result = build_web_search_url(template, instance, "x");
            assert!(
                result.is_ok(),
                "default keyword {keyword:?} failed: {result:?}",
            );
        }
    }
}
