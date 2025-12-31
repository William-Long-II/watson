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

    let file_content = format!("# {}\n\n{}", title, content);
    std::fs::write(&path, file_content).map_err(|e| e.to_string())
}

pub fn delete_note_file(storage_path: &Path, id: &str) -> Result<(), String> {
    // Find and delete the file matching this id
    if let Ok(entries) = std::fs::read_dir(storage_path) {
        let prefix = id.replace("note:", "");
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&prefix) {
                    std::fs::remove_file(entry.path()).ok();
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .chars()
        .take(50)
        .collect()
}
