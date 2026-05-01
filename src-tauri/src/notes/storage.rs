use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio::fs;

pub async fn write_note_file(
    storage_path: &Path,
    id: &str,
    title: &str,
    content: &str,
) -> Result<(), String> {
    let safe_title = sanitize_filename(title);
    let filename = format!("{}-{}.md", id.replace("note:", ""), safe_title);
    let path = storage_path.join(&filename);

    // R-05 mitigation: if a prior version of this note was written with a
    // different title, its .md file remains under the old filename. Before
    // writing the new file, clear any stale variants so the invariant
    // "exactly one .md file per note id" holds.
    cleanup_stale_files_for_id(storage_path, id, &filename).await;

    // R-05 mitigation: write atomically. A crash between the two lines of
    // std::fs::write can leave a truncated/empty file under `path`. Writing
    // to a .tmp sibling and renaming guarantees a reader sees either the
    // old complete contents or the new complete contents — never a
    // half-written file. On the same filesystem, std::fs::rename is atomic.
    let file_content = format!("# {}\n\n{}", title, content);
    let tmp_path = storage_path.join(format!("{filename}.tmp"));
    fs::write(&tmp_path, file_content).await.map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&tmp_path, &path).await {
        // Best-effort cleanup of the tmp if the rename failed (e.g., target
        // on a different filesystem). Let the original error surface.
        let _ = fs::remove_file(&tmp_path).await;
        return Err(e.to_string());
    }
    Ok(())
}

pub async fn delete_note_file(storage_path: &Path, id: &str) -> Result<(), String> {
    // Delete ALL files matching this id's prefix. The invariant is one file
    // per id, but any stale orphan gets cleaned up here defensively.
    if let Ok(mut entries) = fs::read_dir(storage_path).await {
        let prefix = id.replace("note:", "");
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&prefix) {
                    fs::remove_file(entry.path()).await.ok();
                }
            }
        }
    }
    Ok(())
}

/// Remove any .md files for this note id EXCEPT the one we're about to write.
/// Called just before `std::fs::write` creates the new file, so the file we
/// keep may not yet exist on disk; we use filename comparison only.
async fn cleanup_stale_files_for_id(storage_path: &Path, id: &str, keep_filename: &str) {
    let Ok(mut entries) = fs::read_dir(storage_path).await else {
        return;
    };
    let prefix = id.replace("note:", "");
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with(&prefix) && name != keep_filename {
            fs::remove_file(entry.path()).await.ok();
        }
    }
}

/// Locate the single .md file for this note id, if one exists. Returns
/// the first match — the invariant enforced by `cleanup_stale_files_for_id`
/// is "exactly one file per id", so any match is the right one.
pub async fn find_note_file(storage_path: &Path, id: &str) -> Option<PathBuf> {
    let prefix = id.replace("note:", "");
    let mut entries = fs::read_dir(storage_path).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".md"))
        {
            return Some(entry.path());
        }
    }
    None
}

/// WAT-204: read back a note file and split it into title + body.
/// Format contract (see `write_note_file`): first line is `# <title>`,
/// followed by a blank line, then the body. If an external editor
/// stripped the header, the whole file becomes the body and the title
/// defaults to empty — the caller decides how to present this.
pub async fn read_note_file(path: &Path) -> Result<(String, String), String> {
    let raw = fs::read_to_string(path).await.map_err(|e| e.to_string())?;
    Ok(parse_note_file(&raw))
}

fn parse_note_file(raw: &str) -> (String, String) {
    // Strip a single leading "# " from the first line; the body is
    // everything after, with ONE optional blank separator removed if
    // present. That separator is what `write_note_file` emits.
    if let Some(rest) = raw.strip_prefix("# ") {
        // Split on first newline to isolate the title line.
        if let Some((title_line, after_title)) = rest.split_once('\n') {
            let title = title_line.trim_end_matches('\r').to_string();
            // Drop ONE blank separator line if present.
            let body = after_title.strip_prefix('\n').unwrap_or(after_title);
            return (title, body.to_string());
        } else {
            // File is just "# title" with no body.
            return (rest.trim_end_matches('\r').to_string(), String::new());
        }
    }
    // No header — treat entire file as body; title is empty so the UI
    // can prompt the user for one.
    (String::new(), raw.to_string())
}

/// Return file mtime as unix seconds. `None` if the file doesn't exist
/// or the filesystem doesn't report an mtime.
pub async fn file_modified_at(path: &Path) -> Option<i64> {
    let meta = fs::metadata(path).await.ok()?;
    let mtime = meta.modified().ok()?;
    let secs = mtime.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(secs as i64)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .chars()
        .take(50)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_note_file ---

    #[test]
    fn parse_splits_title_and_body_on_standard_format() {
        let raw = "# Meeting Notes\n\nDiscussed roadmap.";
        let (title, body) = parse_note_file(raw);
        assert_eq!(title, "Meeting Notes");
        assert_eq!(body, "Discussed roadmap.");
    }

    #[test]
    fn parse_handles_multiline_body() {
        let raw = "# T\n\nLine 1\nLine 2\n\nLine 4";
        let (_, body) = parse_note_file(raw);
        assert_eq!(body, "Line 1\nLine 2\n\nLine 4");
    }

    #[test]
    fn parse_handles_title_without_body() {
        let raw = "# Lonely";
        let (title, body) = parse_note_file(raw);
        assert_eq!(title, "Lonely");
        assert_eq!(body, "");
    }

    #[test]
    fn parse_handles_missing_header() {
        // An external editor may have removed the "# title" line. Present
        // the whole thing as body; caller can prompt for a title.
        let raw = "no header\njust body";
        let (title, body) = parse_note_file(raw);
        assert_eq!(title, "");
        assert_eq!(body, "no header\njust body");
    }

    #[test]
    fn parse_tolerates_crlf_line_endings() {
        // Windows editors write \r\n. Title line shouldn't include the \r.
        let raw = "# Title\r\n\r\nbody\r\n";
        let (title, _) = parse_note_file(raw);
        assert_eq!(title, "Title");
    }

    #[test]
    fn parse_preserves_body_when_no_blank_separator() {
        // Defensive: user wrote "# Title\nbody" without the blank line.
        // Body starts directly after the title line.
        let raw = "# T\nbody";
        let (_, body) = parse_note_file(raw);
        assert_eq!(body, "body");
    }

    // --- file_modified_at ---

    #[tokio::test]
    async fn file_modified_at_returns_some_for_existing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("t.md");
        fs::write(&path, "x").await.unwrap();
        assert!(file_modified_at(&path).await.is_some());
    }

    #[tokio::test]
    async fn file_modified_at_is_none_for_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(file_modified_at(&dir.path().join("nope.md")).await.is_none());
    }

    // --- find_note_file ---

    #[tokio::test]
    async fn find_note_file_locates_by_id_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join("42-Meeting.md"), "# Meeting\n\nbody").await.unwrap();
        let found = find_note_file(dir.path(), "note:42").await.unwrap();
        assert!(found.ends_with("42-Meeting.md"));
    }

    #[tokio::test]
    async fn find_note_file_returns_none_when_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(find_note_file(dir.path(), "note:999").await.is_none());
    }

    #[tokio::test]
    async fn find_note_file_ignores_non_md_files() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join("42-garbage.txt"), "x").await.unwrap();
        assert!(find_note_file(dir.path(), "note:42").await.is_none());
    }
}
