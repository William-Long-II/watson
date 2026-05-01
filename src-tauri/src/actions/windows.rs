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

#[cfg(target_os = "linux")]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    linux::get_open_windows()
}

#[cfg(target_os = "linux")]
pub fn focus_window(hwnd: i64) -> Result<(), String> {
    linux::focus_window(hwnd)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
    Ok(vec![])
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn focus_window(_hwnd: i64) -> Result<(), String> {
    Err("Window focusing not yet implemented on this platform".to_string())
}

// --- Linux backend ---
//
// Two display servers, two stories.
//
// X11 (P0): EWMH gives us everything we need. `_NET_CLIENT_LIST` on
// the root window is the canonical "list of top-level windows
// managed by the WM" — same role `EnumWindows` plays on Win32.
// Activation is `_NET_ACTIVE_WINDOW` ClientMessage with source=2
// ("pager / window switcher"), which compositors trust to bypass
// focus-stealing prevention. Pure-Rust x11rb keeps libxcb out of
// the build dep tree.
//
// Wayland (P1, follow-up issue): there is no portable cross-app
// enumeration protocol — by design. wlr-foreign-toplevel works on
// wlroots-based compositors (Sway, Hyprland, river); GNOME/Mutter
// and KDE/KWin have no usable equivalent. Until the wlr port
// lands, Wayland sessions return empty enumeration + an
// explanatory error from focus_window. Detection is by
// `XDG_SESSION_TYPE`.
#[cfg(target_os = "linux")]
mod linux {
    use super::WindowEntry;

    enum Backend {
        X11,
        Wayland(String),
    }

    fn detect_backend() -> Backend {
        let session_type = std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if session_type == "wayland" {
            let name = std::env::var("XDG_CURRENT_DESKTOP")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Wayland".to_string());
            Backend::Wayland(name)
        } else {
            // x11, tty, or missing — try X11. If there's no X server
            // the connect call returns Err and the caller gets a
            // clean error string instead of a silent empty result.
            Backend::X11
        }
    }

    pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
        match detect_backend() {
            Backend::X11 => x11::get_open_windows(),
            Backend::Wayland(_) => Ok(Vec::new()),
        }
    }

    pub fn focus_window(hwnd: i64) -> Result<(), String> {
        match detect_backend() {
            Backend::X11 => x11::focus_window(hwnd),
            Backend::Wayland(name) => Err(format!(
                "Window switching is not supported on {name} (Wayland session). \
                 Log in with an X11 session to use the switcher; \
                 Wayland support is tracked in a separate issue."
            )),
        }
    }

    /// Resolve a process name from a PID via `/proc/<pid>/comm`,
    /// falling back to the `/proc/<pid>/exe` symlink basename, then
    /// to "unknown". Sandboxed browsers (Snap, Flatpak) may block
    /// reads from outside their namespace; treat errors as "unknown".
    fn process_name_for_pid(pid: u32) -> String {
        if let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            let trimmed = comm.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        if let Ok(path) = std::fs::read_link(format!("/proc/{pid}/exe")) {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
        "unknown".to_string()
    }

    mod x11 {
        use super::process_name_for_pid;
        use super::super::WindowEntry;
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{
            AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
        };

        /// Pre-interned atoms. Atoms don't change after server
        /// startup so we cache them per `enumerate` call (creating an
        /// X11 connection is itself a syscall round-trip; one set of
        /// atom interns adds negligible cost on top).
        struct Atoms {
            net_client_list: u32,
            net_wm_name: u32,
            net_wm_pid: u32,
            net_wm_window_type: u32,
            net_wm_state: u32,
            net_active_window: u32,
            utf8_string: u32,
            // Window types we explicitly reject (the rest are kept).
            type_desktop: u32,
            type_dock: u32,
            type_toolbar: u32,
            type_menu: u32,
            type_splash: u32,
            type_notification: u32,
            type_dropdown_menu: u32,
            type_popup_menu: u32,
            type_tooltip: u32,
            type_combo: u32,
            type_dnd: u32,
            // States we reject.
            state_hidden: u32,
        }

        impl Atoms {
            fn intern<C: Connection>(conn: &C) -> Result<Self, String> {
                let intern = |name: &[u8]| -> Result<u32, String> {
                    conn.intern_atom(false, name)
                        .map_err(|e| format!("intern_atom({}): {e}", String::from_utf8_lossy(name)))?
                        .reply()
                        .map(|r| r.atom)
                        .map_err(|e| format!("intern_atom reply: {e}"))
                };
                Ok(Self {
                    net_client_list: intern(b"_NET_CLIENT_LIST")?,
                    net_wm_name: intern(b"_NET_WM_NAME")?,
                    net_wm_pid: intern(b"_NET_WM_PID")?,
                    net_wm_window_type: intern(b"_NET_WM_WINDOW_TYPE")?,
                    net_wm_state: intern(b"_NET_WM_STATE")?,
                    net_active_window: intern(b"_NET_ACTIVE_WINDOW")?,
                    utf8_string: intern(b"UTF8_STRING")?,
                    type_desktop: intern(b"_NET_WM_WINDOW_TYPE_DESKTOP")?,
                    type_dock: intern(b"_NET_WM_WINDOW_TYPE_DOCK")?,
                    type_toolbar: intern(b"_NET_WM_WINDOW_TYPE_TOOLBAR")?,
                    type_menu: intern(b"_NET_WM_WINDOW_TYPE_MENU")?,
                    type_splash: intern(b"_NET_WM_WINDOW_TYPE_SPLASH")?,
                    type_notification: intern(b"_NET_WM_WINDOW_TYPE_NOTIFICATION")?,
                    type_dropdown_menu: intern(b"_NET_WM_WINDOW_TYPE_DROPDOWN_MENU")?,
                    type_popup_menu: intern(b"_NET_WM_WINDOW_TYPE_POPUP_MENU")?,
                    type_tooltip: intern(b"_NET_WM_WINDOW_TYPE_TOOLTIP")?,
                    type_combo: intern(b"_NET_WM_WINDOW_TYPE_COMBO")?,
                    type_dnd: intern(b"_NET_WM_WINDOW_TYPE_DND")?,
                    state_hidden: intern(b"_NET_WM_STATE_HIDDEN")?,
                })
            }

            fn is_excluded_type(&self, atom: u32) -> bool {
                atom == self.type_desktop
                    || atom == self.type_dock
                    || atom == self.type_toolbar
                    || atom == self.type_menu
                    || atom == self.type_splash
                    || atom == self.type_notification
                    || atom == self.type_dropdown_menu
                    || atom == self.type_popup_menu
                    || atom == self.type_tooltip
                    || atom == self.type_combo
                    || atom == self.type_dnd
            }
        }

        /// Read a property as an array of 32-bit values (CARDINAL,
        /// WINDOW, ATOM all share this representation in the wire
        /// protocol).
        fn get_prop_u32<C: Connection>(
            conn: &C,
            window: Window,
            property: u32,
            type_: impl Into<u32>,
        ) -> Option<Vec<u32>> {
            let reply = conn
                .get_property(false, window, property, type_, 0, u32::MAX / 4)
                .ok()?
                .reply()
                .ok()?;
            if reply.format != 32 {
                return None;
            }
            // value is little-endian on all current X11 servers we
            // care about (we connect locally; protocol byte-order is
            // negotiated client-side, x11rb selects native).
            Some(
                reply
                    .value
                    .chunks_exact(4)
                    .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            )
        }

        /// Read a UTF-8 property; falls back to legacy COMPOUND_TEXT
        /// / STRING (`WM_NAME`) when `_NET_WM_NAME` is missing. Returns
        /// `None` if the window has no usable title.
        fn get_window_title<C: Connection>(
            conn: &C,
            window: Window,
            atoms: &Atoms,
        ) -> Option<String> {
            // _NET_WM_NAME (UTF8_STRING) — preferred.
            let cookie = conn
                .get_property(false, window, atoms.net_wm_name, atoms.utf8_string, 0, u32::MAX / 4)
                .ok()?;
            if let Ok(reply) = cookie.reply() {
                if !reply.value.is_empty() {
                    if let Ok(s) = String::from_utf8(reply.value) {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
            // WM_NAME — legacy ICCCM property; type is STRING (Latin-1).
            let cookie = conn
                .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, u32::MAX / 4)
                .ok()?;
            if let Ok(reply) = cookie.reply() {
                if !reply.value.is_empty() {
                    let s: String = reply.value.into_iter().map(char::from).collect();
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
            None
        }

        pub fn get_open_windows() -> Result<Vec<WindowEntry>, String> {
            let (conn, screen_num) = x11rb::connect(None)
                .map_err(|e| format!("X11 connect failed: {e}"))?;
            let root = conn.setup().roots[screen_num].root;
            let atoms = Atoms::intern(&conn)?;

            // _NET_CLIENT_LIST is type WINDOW (atom). x11rb's typed
            // AtomEnum variant for WINDOW lives on AtomEnum::WINDOW.
            let client_list = get_prop_u32(&conn, root, atoms.net_client_list, AtomEnum::WINDOW)
                .ok_or_else(|| {
                    "_NET_CLIENT_LIST missing on root — window manager doesn't \
                     advertise EWMH support, switcher unavailable"
                        .to_string()
                })?;

            let our_pid = std::process::id();
            let mut entries = Vec::with_capacity(client_list.len());

            for w in client_list {
                // Window type filter. Missing _NET_WM_WINDOW_TYPE is
                // treated as NORMAL per EWMH §Application Window
                // Properties.
                if let Some(types) = get_prop_u32(&conn, w, atoms.net_wm_window_type, AtomEnum::ATOM) {
                    if types.iter().any(|&t| atoms.is_excluded_type(t)) {
                        continue;
                    }
                }

                // _NET_WM_STATE filter — drop minimized windows so the
                // switcher doesn't list things the user explicitly
                // hid. (We could surface them with a marker, but
                // matches Win32 behavior for now.)
                if let Some(states) = get_prop_u32(&conn, w, atoms.net_wm_state, AtomEnum::ATOM) {
                    if states.contains(&atoms.state_hidden) {
                        continue;
                    }
                }

                let title = match get_window_title(&conn, w, &atoms) {
                    Some(t) => t,
                    None => continue, // Skip untitled windows (matches Win32).
                };

                let pid = get_prop_u32(&conn, w, atoms.net_wm_pid, AtomEnum::CARDINAL)
                    .and_then(|v| v.first().copied())
                    .unwrap_or(0);

                if pid != 0 && pid == our_pid {
                    continue; // Skip Watson's own window.
                }

                let process_name = if pid == 0 {
                    "unknown".to_string()
                } else {
                    process_name_for_pid(pid)
                };

                entries.push(WindowEntry {
                    hwnd: w as i64,
                    pid,
                    process_name,
                    title,
                });
            }

            Ok(entries)
        }

        pub fn focus_window(hwnd: i64) -> Result<(), String> {
            // Window IDs in X11 are 32-bit. Reject anything that
            // can't possibly be a valid handle before opening the
            // connection.
            if hwnd <= 0 || hwnd > u32::MAX as i64 {
                return Err(format!("invalid X11 window id: {hwnd}"));
            }
            let target: Window = hwnd as u32;

            let (conn, screen_num) = x11rb::connect(None)
                .map_err(|e| format!("X11 connect failed: {e}"))?;
            let root = conn.setup().roots[screen_num].root;
            let atoms = Atoms::intern(&conn)?;

            // Sanity-check the window still exists. get_geometry is
            // the cheapest existence probe; a closed window returns a
            // BadWindow error which x11rb surfaces as Err.
            conn.get_geometry(target)
                .map_err(|e| format!("get_geometry({hwnd:#x}): {e}"))?
                .reply()
                .map_err(|_| format!("window {hwnd:#x} no longer exists"))?;

            // EWMH §_NET_ACTIVE_WINDOW: data.l[0]=2 indicates source
            // is a pager / window switcher; data.l[1] is the
            // timestamp (0 = CurrentTime is acceptable for source=2);
            // data.l[2] is the requestor's currently-active window
            // (0 if none).
            let event = ClientMessageEvent::new(32, target, atoms.net_active_window, [2u32, 0, 0, 0, 0]);
            conn.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|e| format!("send_event: {e}"))?;
            // Belt-and-suspenders: also raise. Most EWMH-compliant
            // WMs will raise as part of activate, but minimal WMs
            // (twm-likes, tiling WMs in floating mode) sometimes
            // don't. configure_window with above-sibling=None lifts
            // to the top of its sibling stack.
            use x11rb::protocol::xproto::{ConfigureWindowAux, StackMode};
            let _ = conn
                .configure_window(target, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));

            conn.flush().map_err(|e| format!("flush: {e}"))?;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn detect_backend_wayland_session_with_desktop() {
            // Save + restore so test ordering with detect_backend_x11
            // doesn't leak. std::env::set_var is unsafe in newer Rust
            // editions; we accept the test-only race risk.
            let prior_st = std::env::var("XDG_SESSION_TYPE").ok();
            let prior_de = std::env::var("XDG_CURRENT_DESKTOP").ok();
            std::env::set_var("XDG_SESSION_TYPE", "wayland");
            std::env::set_var("XDG_CURRENT_DESKTOP", "GNOME");
            match detect_backend() {
                Backend::Wayland(name) => assert_eq!(name, "GNOME"),
                _ => panic!("expected Wayland(GNOME)"),
            }
            match prior_st {
                Some(v) => std::env::set_var("XDG_SESSION_TYPE", v),
                None => std::env::remove_var("XDG_SESSION_TYPE"),
            }
            match prior_de {
                Some(v) => std::env::set_var("XDG_CURRENT_DESKTOP", v),
                None => std::env::remove_var("XDG_CURRENT_DESKTOP"),
            }
        }

        #[test]
        fn detect_backend_x11_when_session_type_x11() {
            let prior = std::env::var("XDG_SESSION_TYPE").ok();
            std::env::set_var("XDG_SESSION_TYPE", "x11");
            assert!(matches!(detect_backend(), Backend::X11));
            match prior {
                Some(v) => std::env::set_var("XDG_SESSION_TYPE", v),
                None => std::env::remove_var("XDG_SESSION_TYPE"),
            }
        }

        #[test]
        fn process_name_for_self_pid_is_nonempty() {
            // Reading /proc/<our_pid>/comm should always work for the
            // running test process. The exact name isn't load-bearing
            // (cargo test, watson, target/debug/...), only that we
            // return something other than the "unknown" fallback —
            // which would indicate a regression in the /proc reader.
            let name = process_name_for_pid(std::process::id());
            assert!(!name.is_empty());
            assert_ne!(name, "unknown");
        }

        #[test]
        fn process_name_for_invalid_pid_is_unknown() {
            // PID 0 is the swapper/scheduler and never exposes
            // /proc/0; PID 4_000_000_000 is well above any plausible
            // running PID on 32-bit pid_max systems and almost
            // certainly above 64-bit too. Both should fall through to
            // "unknown".
            assert_eq!(process_name_for_pid(0), "unknown");
            assert_eq!(process_name_for_pid(4_000_000_000), "unknown");
        }
    }
}
