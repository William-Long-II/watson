pub mod system;

/// Reject paths/URLs containing ASCII control characters or nulls.
///
/// Closes R-03: prior `launch_app` on Windows invoked `cmd /C start "" <path>`,
/// which handed the path to cmd.exe where shell metacharacters (`&`, `|`, `^`,
/// `"`) could alter the intended command. The fix replaces cmd.exe with
/// `open::that` (ShellExecuteW on Windows, LaunchServices on macOS, xdg-open
/// with argv-only on Linux) — none of those paths pipe through a shell.
///
/// This validator is defense-in-depth: even without shell interpretation, a
/// path with control chars or nulls is never a legitimate filesystem target
/// and almost certainly indicates a corrupted index entry or an injection
/// attempt. Fail fast.
fn validate_target(target: &str) -> Result<(), String> {
    if target.is_empty() {
        return Err("target is empty".into());
    }
    if let Some(c) = target
        .chars()
        .find(|&c| (c as u32) < 0x20 || c == '\x7f')
    {
        return Err(format!(
            "target contains control character U+{:04X}",
            c as u32
        ));
    }
    Ok(())
}

pub fn launch_app(path: &str) -> Result<(), String> {
    validate_target(path)?;
    open::that(path).map_err(|e| e.to_string())
}

pub fn open_url(url: &str) -> Result<(), String> {
    validate_target(url)?;
    open::that(url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- accepted paths ---

    #[test]
    fn accepts_typical_windows_path() {
        assert!(validate_target(
            r"C:\Program Files\Mozilla Firefox\firefox.exe"
        )
        .is_ok());
    }

    #[test]
    fn accepts_typical_macos_path() {
        assert!(validate_target("/Applications/Firefox.app").is_ok());
    }

    #[test]
    fn accepts_typical_linux_path() {
        assert!(validate_target("/usr/bin/firefox").is_ok());
    }

    #[test]
    fn accepts_path_with_spaces() {
        assert!(validate_target("/Applications/My App.app").is_ok());
    }

    #[test]
    fn accepts_unicode_path() {
        assert!(validate_target("/Users/will/documents/café/notes.txt").is_ok());
    }

    #[test]
    fn accepts_https_url() {
        assert!(validate_target("https://www.google.com/search?q=cats").is_ok());
    }

    #[test]
    fn accepts_path_with_shell_metacharacters() {
        // These characters are NOT control chars. They used to matter under
        // `cmd /C start` on Windows — that's now gone. Pinning the contract:
        // metacharacters in filenames are legal and must not be rejected.
        for path in [
            r"C:\tmp\a & b.exe",
            r"C:\tmp\a|b.exe",
            r"C:\tmp\a^b.exe",
            r#"C:\tmp\a"b.exe"#,
        ] {
            assert!(
                validate_target(path).is_ok(),
                "expected {path:?} to be accepted — it is a legal filename"
            );
        }
    }

    // --- rejected inputs ---

    #[test]
    fn rejects_empty_string() {
        assert!(validate_target("").is_err());
    }

    #[test]
    fn rejects_null_byte() {
        // Ahead of the first real character, between, and trailing.
        for s in ["\0", "C:\\a\0b", "payload\0"] {
            assert!(validate_target(s).is_err(), "expected rejection for {s:?}");
        }
    }

    #[test]
    fn rejects_newline_and_carriage_return() {
        for s in ["/usr/bin/firefox\n", "C:\\app\r\nEVIL"] {
            assert!(validate_target(s).is_err(), "expected rejection for {s:?}");
        }
    }

    #[test]
    fn rejects_tab() {
        assert!(validate_target("C:\\a\tb.exe").is_err());
    }

    #[test]
    fn rejects_bel_and_other_low_control_chars() {
        // 0x07 BEL, 0x1B ESC, 0x1F US — all legal in neither paths nor URLs.
        for ch in ['\x07', '\x1b', '\x1f'] {
            let s = format!("/usr/bin/{ch}firefox");
            assert!(
                validate_target(&s).is_err(),
                "expected rejection for control char {:?}",
                ch as u32
            );
        }
    }

    #[test]
    fn rejects_del() {
        assert!(validate_target("/usr/bin/firefox\x7f").is_err());
    }

    #[test]
    fn error_message_identifies_offending_codepoint() {
        // Operator-facing diagnostics — when a real path gets rejected, the
        // error must be specific enough to investigate. Stable enough to pin.
        let err = validate_target("a\x07b").unwrap_err();
        assert!(
            err.contains("U+0007"),
            "error should mention the codepoint, got: {err:?}"
        );
    }
}
