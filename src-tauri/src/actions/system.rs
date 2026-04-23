use std::process::Command;

#[derive(Debug, Clone)]
pub struct SystemCommand {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub requires_confirmation: bool,
}

pub fn get_system_commands() -> Vec<SystemCommand> {
    vec![
        SystemCommand {
            id: "cmd:lock".to_string(),
            name: "Lock".to_string(),
            aliases: vec!["lock".to_string(), "lockscreen".to_string()],
            description: "Lock the screen".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:sleep".to_string(),
            name: "Sleep".to_string(),
            aliases: vec!["sleep".to_string()],
            description: "Put computer to sleep".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:restart".to_string(),
            name: "Restart".to_string(),
            aliases: vec!["restart".to_string(), "reboot".to_string()],
            description: "Restart the computer".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:shutdown".to_string(),
            name: "Shutdown".to_string(),
            aliases: vec!["shutdown".to_string(), "poweroff".to_string()],
            description: "Shut down the computer".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:logout".to_string(),
            name: "Log Out".to_string(),
            aliases: vec!["logout".to_string(), "signout".to_string()],
            description: "Log out current user".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:emptytrash".to_string(),
            name: "Empty Trash".to_string(),
            aliases: vec!["emptytrash".to_string(), "trash".to_string()],
            description: "Empty the trash/recycle bin".to_string(),
            requires_confirmation: true,
        },
        SystemCommand {
            id: "cmd:mute".to_string(),
            name: "Mute".to_string(),
            aliases: vec!["mute".to_string()],
            description: "Mute system audio".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:unmute".to_string(),
            name: "Unmute".to_string(),
            aliases: vec!["unmute".to_string()],
            description: "Unmute system audio".to_string(),
            requires_confirmation: false,
        },
        // WAT-302: window management. Aliases biased toward what a user
        // would type under `>` — "split", "left", "max", "center".
        SystemCommand {
            id: "cmd:win-left".to_string(),
            name: "Snap Window Left".to_string(),
            aliases: vec![
                "split-left".to_string(),
                "split left".to_string(),
                "snap-left".to_string(),
                "left".to_string(),
                "win-left".to_string(),
            ],
            description: "Snap the active window to the left half of its monitor".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:win-right".to_string(),
            name: "Snap Window Right".to_string(),
            aliases: vec![
                "split-right".to_string(),
                "split right".to_string(),
                "snap-right".to_string(),
                "right".to_string(),
                "win-right".to_string(),
            ],
            description: "Snap the active window to the right half of its monitor".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:win-max".to_string(),
            name: "Maximize Window".to_string(),
            aliases: vec![
                "max".to_string(),
                "maximize".to_string(),
                "win-max".to_string(),
            ],
            description: "Maximize the active window on its current monitor".to_string(),
            requires_confirmation: false,
        },
        SystemCommand {
            id: "cmd:win-center".to_string(),
            name: "Center Window".to_string(),
            aliases: vec![
                "center".to_string(),
                "centre".to_string(),
                "win-center".to_string(),
            ],
            description: "Center the active window on its current monitor".to_string(),
            requires_confirmation: false,
        },
    ]
}

// WAT-302: window-management commands. Separate entry point so the
// per-platform `execute_command` switches can delegate without
// repeating the command-id match. Returns `Ok(true)` when the id was
// handled (even if the action itself failed on an unsupported
// platform), `Ok(false)` when the id wasn't a WAT-302 command so the
// caller should try its other branches.
#[cfg(target_os = "windows")]
fn try_handle_window_command(command_id: &str) -> Result<bool, String> {
    use super::window::WindowAction;
    let Some(action) = WindowAction::from_command_id(command_id) else {
        return Ok(false);
    };
    // Windows' native Win+arrow snap keys handle left/right/max
    // perfectly, including across multiple monitors and respecting the
    // taskbar. SendKeys is a lighter touch than SetWindowPos via
    // P/Invoke. `{LWIN down}…{LWIN up}` is the documented pattern;
    // one-shot key chords (`^{LEFT}`) wouldn't hit the Win modifier.
    let script = match action {
        WindowAction::SplitLeft => {
            r#"(New-Object -ComObject WScript.Shell).SendKeys('{LWIN down}{LEFT}{LWIN up}')"#
        }
        WindowAction::SplitRight => {
            r#"(New-Object -ComObject WScript.Shell).SendKeys('{LWIN down}{RIGHT}{LWIN up}')"#
        }
        WindowAction::Maximize => {
            r#"(New-Object -ComObject WScript.Shell).SendKeys('{LWIN down}{UP}{LWIN up}')"#
        }
        // Center: no native key chord, so call the Win32 API directly
        // through a PowerShell Add-Type shim. We pick an 80% × 80%
        // window size centered on the work area for consistency with
        // typical "reset layout" bindings in other launchers.
        WindowAction::Center => WINDOW_CENTER_PS_SCRIPT,
    };
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(not(target_os = "windows"))]
fn try_handle_window_command(command_id: &str) -> Result<bool, String> {
    use super::window::WindowAction;
    if WindowAction::from_command_id(command_id).is_none() {
        return Ok(false);
    }
    // Handled-but-unsupported. Return Ok(true) so the caller doesn't
    // also try to dispatch as a legacy command, but the inner Err
    // surfaces the platform gap to the UI.
    Err(format!(
        "Window management is not yet implemented on this platform (command: {command_id}). \
         Follow-up tickets will add macOS (accessibility API) and Linux (wmctrl) support."
    ))
}

#[cfg(target_os = "windows")]
const WINDOW_CENTER_PS_SCRIPT: &str = r#"
Add-Type -Name WinApi -Namespace Watson -MemberDefinition @"
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
[DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint dwFlags);
[DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO mi);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
[StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
"@
$hwnd = [Watson.WinApi]::GetForegroundWindow()
$mon  = [Watson.WinApi]::MonitorFromWindow($hwnd, 2)  # MONITOR_DEFAULTTONEAREST
$mi   = New-Object Watson.WinApi+MONITORINFO
$mi.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($mi)
[void][Watson.WinApi]::GetMonitorInfo($mon, [ref]$mi)
$work = $mi.rcWork
$mw = $work.Right - $work.Left
$mh = $work.Bottom - $work.Top
# 80% × 80% window size is the Watson convention for `center` — a
# visually obvious "reset" from any prior snap/max state.
$ww = [int]($mw * 0.8)
$wh = [int]($mh * 0.8)
$x  = $work.Left + [int](($mw - $ww) / 2)
$y  = $work.Top  + [int](($mh - $wh) / 2)
[void][Watson.WinApi]::MoveWindow($hwnd, $x, $y, $ww, $wh, $true)
"#;

#[cfg(target_os = "macos")]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    // WAT-302: window commands are handled here first. Unsupported on
    // macOS for v1 (accessibility API requires permission UX beyond
    // this PR's scope); the helper returns Err directly.
    if try_handle_window_command(command_id)? {
        return Ok(());
    }
    match command_id {
        "cmd:lock" => {
            Command::new("pmset")
                .args(["displaysleepnow"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:sleep" => {
            Command::new("pmset")
                .args(["sleepnow"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:restart" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to restart"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:shutdown" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to shut down"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:logout" => {
            Command::new("osascript")
                .args(["-e", "tell app \"System Events\" to log out"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:emptytrash" => {
            Command::new("osascript")
                .args(["-e", "tell app \"Finder\" to empty the trash"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:mute" => {
            Command::new("osascript")
                .args(["-e", "set volume with output muted"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:unmute" => {
            Command::new("osascript")
                .args(["-e", "set volume without output muted"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown command: {}", command_id)),
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    // WAT-302: window commands come first because they short-circuit
    // the legacy-command match below on success.
    if try_handle_window_command(command_id)? {
        return Ok(());
    }
    match command_id {
        "cmd:lock" => {
            Command::new("rundll32.exe")
                .args(["user32.dll,LockWorkStation"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:sleep" => {
            Command::new("rundll32.exe")
                .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:restart" => {
            Command::new("shutdown")
                .args(["/r", "/t", "0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:shutdown" => {
            Command::new("shutdown")
                .args(["/s", "/t", "0"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:logout" => {
            Command::new("shutdown")
                .args(["/l"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:emptytrash" => {
            Command::new("powershell")
                .args(["-Command", "Clear-RecycleBin -Force"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:mute" => {
            Command::new("powershell")
                .args(["-Command", "(New-Object -ComObject WScript.Shell).SendKeys([char]173)"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        "cmd:unmute" => {
            Command::new("powershell")
                .args(["-Command", "(New-Object -ComObject WScript.Shell).SendKeys([char]173)"])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("Unknown command: {}", command_id)),
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn execute_command(command_id: &str) -> Result<(), String> {
    // WAT-302: surface a clean "not yet supported" error for window
    // commands on Linux; the legacy fallback below handles everything
    // else.
    if try_handle_window_command(command_id)? {
        return Ok(());
    }
    Err(format!("System commands not supported on this platform: {}", command_id))
}
