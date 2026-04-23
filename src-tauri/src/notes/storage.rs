use std::path::Path;

pub fn write_note_file(
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
    cleanup_stale_files_for_id(storage_path, id, &filename);

    // R-05 mitigation: write atomically. A crash between the two lines of
    // std::fs::write can leave a truncated/empty file under `path`. Writing
    // to a .tmp sibling and renaming guarantees a reader sees either the
    // old complete contents or the new complete contents — never a
    // half-written file. On the same filesystem, std::fs::rename is atomic.
    let file_content = format!("# {}\n\n{}", title, content);
    let tmp_path = storage_path.join(format!("{filename}.tmp"));
    std::fs::write(&tmp_path, file_content).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        // Best-effort cleanup of the tmp if the rename failed (e.g., target
        // on a different filesystem). Let the original error surface.
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e.to_string());
    }
    Ok(())
}

pub fn delete_note_file(storage_path: &Path, id: &str) -> Result<(), String> {
    // Delete ALL files matching this id's prefix. The invariant is one file
    // per id, but any stale orphan gets cleaned up here defensively.
    if let Ok(entries) = std::fs::read_dir(storage_path) {
        let prefix = id.replace("note:", "");
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&prefix) {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
    }
    Ok(())
}

/// Remove any .md files for this note id EXCEPT the one we're about to write.
/// Called just before `std::fs::write` creates the new file, so the file we
/// keep may not yet exist on disk; we use filename comparison only.
fn cleanup_stale_files_for_id(storage_path: &Path, id: &str, keep_filename: &str) {
    let Ok(entries) = std::fs::read_dir(storage_path) else {
        return;
    };
    let prefix = id.replace("note:", "");
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with(&prefix) && name != keep_filename {
            std::fs::remove_file(entry.path()).ok();
        }
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .chars()
        .take(50)
        .collect()
}
