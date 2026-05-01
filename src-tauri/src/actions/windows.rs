use std::process::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEntry {
    pub pid: u32,
    pub process_name: String,
    pub title: String,
}

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    // PowerShell script to list windows with titles.
    // Filtering out windows with empty titles and specific system processes.
    let script = r#"
        Get-Process | Where-Object { $_.MainWindowTitle } | 
        Select-Object Id, ProcessName, MainWindowTitle | 
        ConvertTo-Json -Compress
    "#;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
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

// --- Direct Win32 FFI for focus_window ---
//
// The previous implementation shelled out to PowerShell to call
// `SetForegroundWindow`. That was fundamentally broken on Windows
// because `SetForegroundWindow` only succeeds when the **calling
// process** is the foreground process — and PowerShell is a separate
// process from Watson, so the call was silently denied. The OS just
// flashed the target window's taskbar icon. The "min-restore" trick
// changes z-order in the background but does NOT grant foreground
// rights, so it didn't help.
//
// The fix: do the dance from Watson's own process. When the user
// presses Enter on a "Switch to…" result, Watson's window IS the
// foreground window — so calling `SetForegroundWindow` from Watson's
// process passes the foreground-rights check and the OS lets us
// hand the foreground to the target.
//
// We additionally use `AttachThreadInput` to attach our calling thread
// to the current foreground thread's input queue. This is the
// canonical workaround documented for cases where the simple call
// fails (Tauri may dispatch IPC handlers off the UI thread, in which
// case our calling thread isn't strictly the foreground thread, just
// part of the foreground process). The attach makes input-queue rules
// treat them as one. Cheap insurance.
#[cfg(target_os = "windows")]
mod ffi {
    use std::os::raw::{c_int, c_void};

    pub type HWND = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LPARAM = isize;

    pub const FALSE: BOOL = 0;
    pub const TRUE: BOOL = 1;
    pub const SW_SHOW: c_int = 5;
    pub const SW_RESTORE: c_int = 9;

    pub type EnumWindowsProc =
        unsafe extern "system" fn(hwnd: HWND, lparam: LPARAM) -> BOOL;

    extern "system" {
        pub fn SetForegroundWindow(hwnd: HWND) -> BOOL;
        pub fn BringWindowToTop(hwnd: HWND) -> BOOL;
        pub fn ShowWindow(hwnd: HWND, n_cmd_show: c_int) -> BOOL;
        pub fn IsIconic(hwnd: HWND) -> BOOL;
        pub fn IsWindowVisible(hwnd: HWND) -> BOOL;
        pub fn GetWindowTextLengthW(hwnd: HWND) -> c_int;
        pub fn EnumWindows(enum_func: EnumWindowsProc, lparam: LPARAM) -> BOOL;
        pub fn GetWindowThreadProcessId(
            hwnd: HWND,
            lpdw_process_id: *mut DWORD,
        ) -> DWORD;
        pub fn GetForegroundWindow() -> HWND;
        pub fn AttachThreadInput(id_attach: DWORD, id_attach_to: DWORD, f_attach: BOOL) -> BOOL;
        pub fn GetCurrentThreadId() -> DWORD;
    }
}

#[cfg(target_os = "windows")]
pub fn focus_window(pid: u32) -> Result<(), String> {
    use ffi::*;

    // Pick the best top-level window for `pid`. Brave / Chrome
    // / Slack-class apps run multiple processes; the one whose PID
    // matched our get_open_windows enumeration may have multiple
    // top-level HWNDs (popups, devtools, splash). We want the first
    // visible window with a non-empty title — the "main" window the
    // user sees in their taskbar.
    let target = unsafe { find_main_window_for_pid(pid) }
        .ok_or_else(|| format!("no visible top-level window found for pid {pid}"))?;

    unsafe {
        let fg_hwnd = GetForegroundWindow();
        let mut fg_pid: DWORD = 0;
        let fg_tid = GetWindowThreadProcessId(fg_hwnd, &mut fg_pid);
        let my_tid = GetCurrentThreadId();

        // Attach when the current thread differs from the foreground
        // thread. AttachThreadInput on the same id is undefined; the
        // self-attach guard avoids that footgun.
        let attached = if fg_tid != 0 && fg_tid != my_tid {
            AttachThreadInput(my_tid, fg_tid, TRUE) == TRUE
        } else {
            false
        };

        // If minimized, restore. Otherwise just bring to front.
        if IsIconic(target) == TRUE {
            ShowWindow(target, SW_RESTORE);
        } else {
            ShowWindow(target, SW_SHOW);
        }

        // Belt + suspenders: BringWindowToTop adjusts z-order;
        // SetForegroundWindow assigns input focus + activation.
        BringWindowToTop(target);
        SetForegroundWindow(target);

        if attached {
            AttachThreadInput(my_tid, fg_tid, FALSE);
        }
    }

    Ok(())
}

/// Enumerate top-level windows; return the first one belonging to
/// `pid` that is visible AND has a non-empty title. The visibility +
/// title filter weeds out hidden helper windows (Chrome's
/// "Chrome_WidgetWin_*" devtools, splash screens, etc.) so we focus
/// the actual user-facing window.
#[cfg(target_os = "windows")]
unsafe fn find_main_window_for_pid(pid: u32) -> Option<ffi::HWND> {
    use ffi::*;

    // Closure-state passed to the enum callback by raw-pointer through
    // LPARAM. Boxed so the address is stable across the enumeration.
    struct EnumState {
        target_pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam as *mut EnumState) };
        let mut window_pid: DWORD = 0;
        GetWindowThreadProcessId(hwnd, &mut window_pid);
        if window_pid != state.target_pid {
            return TRUE; // continue enumeration
        }
        if IsWindowVisible(hwnd) != TRUE {
            return TRUE;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return TRUE;
        }
        state.found = hwnd;
        FALSE // stop enumeration
    }

    let mut state = EnumState {
        target_pid: pid,
        found: std::ptr::null_mut(),
    };
    EnumWindows(enum_proc, &mut state as *mut EnumState as LPARAM);
    if state.found.is_null() {
        None
    } else {
        Some(state.found)
    }
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
