use serde::{Deserialize, Serialize};

/// One switchable top-level window. The `hwnd` is the canonical
/// identifier — multi-window apps (Brave with three browser windows,
/// VS Code with two projects, Slack with two workspaces) produce one
/// `WindowEntry` per window, NOT one per process.
///
/// HWND is a pointer; on 64-bit Windows it's a 64-bit value. We
/// serialize as `i64` because Tauri IPC is JSON and JSON numbers are
/// IEEE-754 doubles — `i64` round-trips safely as long as the address
/// stays under 2^53, which user-mode HWNDs always do (Windows USERVA
/// limit is 47 bits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowEntry {
    /// The window handle, used by `focus_window`.
    pub hwnd: i64,
    /// Owning process id. Kept for diagnostics + the description label;
    /// not used for focus.
    pub pid: u32,
    /// Process exe basename without `.exe` (e.g. "brave", "Code",
    /// "explorer"). Best-effort — falls back to "unknown" if the
    /// process queries fail (race with process exit, AccessDenied
    /// for elevated processes).
    pub process_name: String,
    /// Window title text as the user would see it in their taskbar.
    pub title: String,
}

// --- Direct Win32 FFI ---
//
// Background: the previous implementation shelled out to PowerShell
// (`Get-Process | Where MainWindowTitle ...`). Two problems:
//
// 1. `MainWindowHandle` returns at most ONE window per process.
//    Multi-window apps were collapsed to a single switcher entry.
// 2. `SetForegroundWindow` from a separate process doesn't focus the
//    target — Windows requires the caller to BE the foreground
//    process. We fixed (2) earlier; this module now also fixes (1)
//    by enumerating top-level windows directly via `EnumWindows`.

#[cfg(target_os = "windows")]
mod ffi {
    use std::os::raw::{c_int, c_void};

    pub type HWND = *mut c_void;
    pub type HANDLE = *mut c_void;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LPARAM = isize;
    pub type LONG = i32;
    pub type LPWSTR = *mut u16;

    pub const FALSE: BOOL = 0;
    pub const TRUE: BOOL = 1;
    pub const SW_SHOW: c_int = 5;
    pub const SW_RESTORE: c_int = 9;
    pub const GW_OWNER: u32 = 4;
    pub const GWL_EXSTYLE: c_int = -20;
    pub const WS_EX_TOOLWINDOW: LONG = 0x00000080;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
    pub const DWMWA_CLOAKED: DWORD = 14;

    pub type EnumWindowsProc =
        unsafe extern "system" fn(hwnd: HWND, lparam: LPARAM) -> BOOL;

    extern "system" {
        // user32
        pub fn SetForegroundWindow(hwnd: HWND) -> BOOL;
        pub fn BringWindowToTop(hwnd: HWND) -> BOOL;
        pub fn ShowWindow(hwnd: HWND, n_cmd_show: c_int) -> BOOL;
        pub fn IsIconic(hwnd: HWND) -> BOOL;
        pub fn IsWindow(hwnd: HWND) -> BOOL;
        pub fn IsWindowVisible(hwnd: HWND) -> BOOL;
        pub fn GetWindowTextLengthW(hwnd: HWND) -> c_int;
        pub fn GetWindowTextW(hwnd: HWND, lp_string: LPWSTR, n_max_count: c_int) -> c_int;
        pub fn GetWindow(hwnd: HWND, u_cmd: u32) -> HWND;
        pub fn GetWindowLongW(hwnd: HWND, n_index: c_int) -> LONG;
        pub fn EnumWindows(enum_func: EnumWindowsProc, lparam: LPARAM) -> BOOL;
        pub fn GetWindowThreadProcessId(hwnd: HWND, lpdw_process_id: *mut DWORD) -> DWORD;
        pub fn GetForegroundWindow() -> HWND;
        pub fn AttachThreadInput(id_attach: DWORD, id_attach_to: DWORD, f_attach: BOOL) -> BOOL;
        // kernel32
        pub fn GetCurrentThreadId() -> DWORD;
        pub fn GetCurrentProcessId() -> DWORD;
        pub fn OpenProcess(
            dw_desired_access: DWORD,
            b_inherit_handle: BOOL,
            dw_process_id: DWORD,
        ) -> HANDLE;
        pub fn QueryFullProcessImageNameW(
            h_process: HANDLE,
            dw_flags: DWORD,
            lp_exe_name: LPWSTR,
            lp_dw_size: *mut DWORD,
        ) -> BOOL;
        pub fn CloseHandle(h_object: HANDLE) -> BOOL;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmGetWindowAttribute(
            hwnd: HWND,
            dw_attribute: DWORD,
            pv_attribute: *mut c_void,
            cb_attribute: DWORD,
        ) -> i32;
    }
}

#[cfg(target_os = "windows")]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    use ffi::*;
    use std::os::raw::c_void;

    struct EnumState {
        our_pid: DWORD,
        entries: Vec<WindowEntry>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam as *mut EnumState) };

        // Filter pipeline. Order matters: cheapest checks first so we
        // bail before the more expensive title / process queries.

        // 1. Visible.
        if IsWindowVisible(hwnd) != TRUE {
            return TRUE;
        }

        // 2. Top-level (no owner). Skips dialogs, popups, message-only
        //    helper windows that taskbar conventions also hide.
        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return TRUE;
        }

        // 3. Not a tool window. WS_EX_TOOLWINDOW is the OS's "this is a
        //    floater, not a real app window" marker — system tray
        //    notifiers, IME helpers, etc.
        let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (exstyle & WS_EX_TOOLWINDOW) != 0 {
            return TRUE;
        }

        // 4. Has a title.
        if GetWindowTextLengthW(hwnd) == 0 {
            return TRUE;
        }

        // 5. Not DWM-cloaked. UWP apps (Calculator, Mail, Settings)
        //    routinely show as visible top-level windows but are
        //    cloaked when not foreground; ApplicationFrameHost.exe
        //    leaves stub windows behind. Without this filter the
        //    switcher fills with phantom rows.
        if is_cloaked(hwnd) {
            return TRUE;
        }

        // 6. Don't surface our own window. The user pressing Alt+Space
        //    on a "Switch to Watson" entry would close-then-show
        //    Watson, which is comedy but not what they want.
        let mut pid: DWORD = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == state.our_pid {
            return TRUE;
        }

        let title = read_window_title(hwnd);
        if title.trim().is_empty() {
            return TRUE;
        }
        let process_name =
            read_process_image_name(pid).unwrap_or_else(|| String::from("unknown"));

        state.entries.push(WindowEntry {
            hwnd: hwnd as i64,
            pid,
            process_name,
            title,
        });

        TRUE // continue enumeration
    }

    unsafe fn read_window_title(hwnd: HWND) -> String {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let cap = (len as usize) + 1;
        let mut buf: Vec<u16> = vec![0u16; cap];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), cap as std::os::raw::c_int);
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }

    unsafe fn read_process_image_name(pid: DWORD) -> Option<String> {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if h.is_null() {
            return None;
        }
        let mut buf: [u16; 1024] = [0; 1024];
        let mut size: DWORD = buf.len() as DWORD;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(h);
        if ok != TRUE {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        // Strip path → basename → drop .exe extension.
        let basename = path
            .rsplit(|c: char| c == '\\' || c == '/')
            .next()
            .unwrap_or(&path);
        let stem = basename
            .strip_suffix(".exe")
            .or_else(|| basename.strip_suffix(".EXE"))
            .unwrap_or(basename);
        Some(stem.to_string())
    }

    unsafe fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked: u32 = 0;
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut c_void,
            std::mem::size_of::<u32>() as DWORD,
        );
        hr == 0 && cloaked != 0
    }

    let mut state = EnumState {
        our_pid: unsafe { GetCurrentProcessId() },
        entries: Vec::new(),
    };
    unsafe {
        EnumWindows(enum_proc, &mut state as *mut EnumState as LPARAM);
    }
    Ok(state.entries)
}

#[cfg(target_os = "windows")]
pub fn focus_window(hwnd: i64) -> Result<(), String> {
    use ffi::*;

    let target = hwnd as HWND;

    // The HWND can be stale by the time we get here — the user opened
    // the picker, took a coffee break, came back, the target window
    // was closed. Validate before doing anything.
    unsafe {
        if IsWindow(target) != TRUE {
            return Err(format!("window {hwnd:#x} no longer exists"));
        }
    }

    unsafe {
        let fg_hwnd = GetForegroundWindow();
        let mut fg_pid: DWORD = 0;
        let fg_tid = GetWindowThreadProcessId(fg_hwnd, &mut fg_pid);
        let my_tid = GetCurrentThreadId();

        // Belt-and-suspenders: attach our calling thread to the
        // foreground thread's input queue. Tauri may dispatch IPC
        // handlers off the UI thread, so the calling thread isn't
        // strictly the foreground thread (only the foreground PROCESS
        // matches). The attach makes input-queue rules treat them as
        // one. Self-attach on Windows is undefined; the guard avoids
        // it.
        let attached = if fg_tid != 0 && fg_tid != my_tid {
            AttachThreadInput(my_tid, fg_tid, TRUE) == TRUE
        } else {
            false
        };

        if IsIconic(target) == TRUE {
            ShowWindow(target, SW_RESTORE);
        } else {
            ShowWindow(target, SW_SHOW);
        }

        BringWindowToTop(target);
        SetForegroundWindow(target);

        if attached {
            AttachThreadInput(my_tid, fg_tid, FALSE);
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    // macOS support is a follow-up. Returning empty keeps the picker
    // pipeline well-behaved (no panics, no error toast); the
    // "Switch to…" feature simply doesn't surface results yet.
    Ok(vec![])
}

#[cfg(target_os = "macos")]
pub fn focus_window(_hwnd: i64) -> Result<(), String> {
    Err("Window focusing not yet implemented on macOS".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    Ok(vec![])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn focus_window(_hwnd: i64) -> Result<(), String> {
    Err("Window focusing not yet implemented on this platform".to_string())
}
