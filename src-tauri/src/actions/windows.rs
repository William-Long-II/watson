use std::process::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEntry {
    pub pid: u32,
    pub process_name: String,
    pub title: String,
}

#[cfg(target_os = "windows")]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    // PowerShell script to list windows with titles.
    // Filtering out windows with empty titles and specific system processes.
    let script = r#"
        Get-Process | Where-Object { $_.MainWindowTitle } | 
        Select-Object Id, ProcessName, MainWindowTitle | 
        ConvertTo-Json -Compress
    "#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(vec![]);
    }

    // PowerShell returns a single object if there's only one result,
    // or an array if there are multiple. We handle both.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PsOutput {
        Single(PsWindow),
        Multiple(Vec<PsWindow>),
    }

    #[derive(Deserialize)]
    struct PsWindow {
        #[serde(rename = "Id")]
        id: u32,
        #[serde(rename = "ProcessName")]
        process_name: String,
        #[serde(rename = "MainWindowTitle")]
        title: String,
    }

    let ps_data: PsOutput = serde_json::from_str(&stdout).map_err(|e| e.to_string())?;
    let windows = match ps_data {
        PsOutput::Single(w) => vec![w],
        PsOutput::Multiple(ws) => ws,
    };

    Ok(windows
        .into_iter()
        .map(|w| WindowEntry {
            pid: w.id,
            process_name: w.process_name,
            title: w.title,
        })
        .collect())
}

#[cfg(target_os = "windows")]
pub fn focus_window(pid: u32) -> Result<(), String> {
    // Using AppActivate via PowerShell. It's the simplest way to focus by PID
    // without pulling in heavy Win32 dependencies.
    let script = format!(
        "(New-Object -ComObject WScript.Shell).AppActivate({})",
        pid
    );

    Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(target_os = "macos")]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    // AppleScript to list window names and their process names.
    let script = r#"
        tell application "System Events"
            set windowList to {}
            repeat with proc in (every process where background only is false)
                set procName to name of proc
                repeat with win in (every window of proc)
                    set end of windowList to {pid:unix id of proc, procName:procName, title:name of win}
                end repeat
            end repeat
            return windowList
        end tell
    "#;
    // For now, returning empty to avoid complex AppleScript parsing in v1.
    // macOS support is a follow-up as noted in the roadmap.
    Ok(vec![])
}

#[cfg(target_os = "macos")]
pub fn focus_window(_pid: u32) -> Result<(), String> {
    Err("Window focusing not yet implemented on macOS".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    Ok(vec![])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn focus_window(_pid: u32) -> Result<(), String> {
    Err("Window focusing not yet implemented on this platform".to_string())
}
