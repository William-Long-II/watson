//! Browser-tab enumeration via Windows UI Automation (UIA).
//!
//! Why this module exists:
//!
//! Browser tabs are NOT operating-system windows. Brave, Chrome, Edge,
//! and Firefox each render every tab inside a single Win32 HWND using
//! their own internal compositor. `EnumWindows` cannot see tabs — it
//! sees only the parent browser window. The parent's `GetWindowTextW`
//! returns the active tab's title, so a window switcher built on
//! Win32 enumeration alone collapses every browser-window's tabs into
//! a single switcher entry.
//!
//! UIA is Microsoft's accessibility framework. Browsers populate a
//! cross-process accessibility tree describing their UI semantically:
//! each tab strip exposes a sequence of `TabItem` controls with the
//! tab's title as its `Name` property. Screen readers use the same
//! tree. We use it to enumerate tabs and to invoke selection on a
//! specific tab.
//!
//! ## Approach
//!
//! 1. Initialize COM once per thread (apartment-threaded).
//! 2. Get the singleton `IUIAutomation` interface via CoCreateInstance.
//! 3. For a browser window's HWND, get its `IUIAutomationElement` root.
//! 4. Build a `PropertyCondition(ControlType == TabItem)` and
//!    `FindAll` descendants matching it.
//! 5. Read each tab's `Name` for display.
//! 6. To switch: re-find the same tab (by index, since it's stable
//!    within a query), invoke `SelectionItemPattern.Select()` on it,
//!    then `SetForegroundWindow` on the parent so the browser comes
//!    to front with the chosen tab active.
//!
//! ## Performance
//!
//! UIA tree walks cross process boundaries and are slow (~50-200ms
//! per browser window). Cache enumerated tabs per HWND with a 1-second
//! TTL so repeated keystrokes during a single search don't cost a
//! tree walk per row update.
//!
//! ## Coverage
//!
//! Brave / Chrome / Edge share the Chromium AX tree shape — they
//! work identically. Firefox surfaces tabs with the same
//! `ControlType.TabItem` but the tree shape differs slightly; the
//! generic `FindAll` query handles it. Other Chromium variants
//! (Vivaldi, Arc, Opera) inherit from Chromium and should work
//! without per-browser code.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabEntry {
    /// Owning browser window HWND, serialized as i64 for JSON safety
    /// (matches the convention in `actions::windows::WindowEntry`).
    pub window_hwnd: i64,
    /// Process exe basename of the owning window — used in the
    /// description ("Switch to Brave: …").
    pub process_name: String,
    /// Tab name as displayed in the browser tab strip.
    pub name: String,
    /// Stable index within the tab strip at enumeration time. Used by
    /// `focus_browser_tab` to find the same tab again at activation.
    /// If the user reorders tabs between enum and activation, the
    /// wrong tab might be focused — acceptable for v1; UIA Runtime
    /// IDs would be the proper fix.
    pub index: i32,
}

/// Process names that get tab enumeration. Anything else is skipped —
/// most apps don't have tabs and the UIA query would return empty
/// anyway, but the early skip avoids ~50ms/window on the search hot
/// path. Lower-cased for comparison.
const BROWSER_PROCESS_NAMES: &[&str] = &[
    "brave",
    "chrome",
    "msedge",
    "firefox",
    "vivaldi",
    "opera",
    "arc",
    // macOS variants. Process names from `NSRunningApplication.localizedName`
    // include "Safari", "Microsoft Edge", "Brave Browser" — the
    // substring match below catches "edge" / "brave" / "chrome" already,
    // but Safari has no other token so it's listed explicitly.
    "safari",
    "edge",
];

pub fn is_browser_process(process_name: &str) -> bool {
    let lower = process_name.to_ascii_lowercase();
    BROWSER_PROCESS_NAMES.iter().any(|b| lower.contains(b))
}

#[cfg(target_os = "windows")]
pub use win::{focus_browser_tab, get_browser_tabs};

#[cfg(target_os = "macos")]
pub use mac::{focus_browser_tab, get_browser_tabs};

#[cfg(target_os = "linux")]
pub use linux_atspi::{focus_browser_tab, get_browser_tabs};

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn get_browser_tabs(_hwnd: i64, _process_name: &str) -> Result<Vec<TabEntry>, String> {
    Ok(vec![])
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn focus_browser_tab(_hwnd: i64, _index: i32) -> Result<(), String> {
    Err("Browser tab switching not yet implemented on this platform".to_string())
}

// --- Windows UIA implementation ---

#[cfg(target_os = "windows")]
mod win {
    use super::TabEntry;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::time::{Duration, Instant};

    use windows::core::{Interface, BSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
        TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_NamePropertyId,
        UIA_SelectionItemPatternId, UIA_TabItemControlTypeId,
    };

    /// Per-thread cache: HWND → (last_enum_at, tabs).
    /// 1-second TTL keeps the search snappy when the user is typing
    /// without forcing a UIA tree walk on every keystroke.
    thread_local! {
        static TAB_CACHE: RefCell<HashMap<i64, (Instant, Vec<TabEntry>)>>
            = RefCell::new(HashMap::new());
        static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    }

    const CACHE_TTL: Duration = Duration::from_secs(1);

    /// Lazily initialize COM (apartment-threaded; idempotent — calling
    /// twice on the same thread returns S_FALSE which we ignore) and
    /// the UIA singleton. Stores the IUIAutomation in TLS so repeated
    /// queries don't re-create the COM object.
    fn uia_get() -> Result<IUIAutomation, String> {
        UIA.with(|cell| {
            if let Some(existing) = cell.borrow().as_ref() {
                return Ok(existing.clone());
            }
            unsafe {
                // CoInitializeEx returns S_OK on first init,
                // RPC_E_CHANGED_MODE if a different threading model is
                // already set, S_FALSE if already in this mode. We
                // tolerate S_FALSE (already-init) and surface
                // RPC_E_CHANGED_MODE as an error.
                let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                if hr.is_err() {
                    let code = hr.0 as u32;
                    // S_FALSE == 0x00000001: already initialized in this mode. OK.
                    if code != 0x00000001 {
                        return Err(format!(
                            "CoInitializeEx failed: 0x{code:08X}"
                        ));
                    }
                }

                let automation: IUIAutomation =
                    CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                        .map_err(|e| format!("CoCreateInstance(UIA): {e}"))?;
                *cell.borrow_mut() = Some(automation.clone());
                Ok(automation)
            }
        })
    }

    /// Enumerate tabs in the browser window identified by `hwnd`.
    /// Cached for 1 second. Returns [] when the window has no
    /// TabItem controls — empty tabbed browsers, or a non-browser
    /// window we accidentally queried.
    pub fn get_browser_tabs(hwnd: i64, process_name: &str) -> Result<Vec<TabEntry>, String> {
        // Cache lookup: short-circuit if we have a fresh result.
        let cached = TAB_CACHE.with(|cell| {
            cell.borrow()
                .get(&hwnd)
                .filter(|(at, _)| at.elapsed() < CACHE_TTL)
                .map(|(_, v)| v.clone())
        });
        if let Some(tabs) = cached {
            return Ok(tabs);
        }

        let tabs = enumerate_uncached(hwnd, process_name)?;

        TAB_CACHE.with(|cell| {
            cell.borrow_mut().insert(hwnd, (Instant::now(), tabs.clone()));
        });
        Ok(tabs)
    }

    fn enumerate_uncached(hwnd: i64, process_name: &str) -> Result<Vec<TabEntry>, String> {
        let automation = uia_get()?;
        unsafe {
            let root: IUIAutomationElement = automation
                .ElementFromHandle(HWND(hwnd as *mut c_void))
                .map_err(|e| format!("ElementFromHandle: {e}"))?;

            // Build: ControlType == TabItem.
            let condition = automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_TabItemControlTypeId.0),
                )
                .map_err(|e| format!("CreatePropertyCondition: {e}"))?;

            let elements = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|e| format!("FindAll: {e}"))?;

            let count = elements.Length().unwrap_or(0);
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count {
                let element = match elements.GetElement(i) {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let name: BSTR = element.CurrentName().unwrap_or_default();
                let name_string = name.to_string();
                if name_string.trim().is_empty() {
                    continue;
                }
                out.push(TabEntry {
                    window_hwnd: hwnd,
                    process_name: process_name.to_string(),
                    name: name_string,
                    index: i,
                });
            }
            Ok(out)
        }
    }

    /// Switch to the tab at `index` in the browser window `hwnd`.
    /// Re-enumerates so a stale cache doesn't operate on a moved tab,
    /// then invokes `SelectionItemPattern.Select()` and brings the
    /// parent window to front so the user actually sees the activation.
    pub fn focus_browser_tab(hwnd: i64, index: i32) -> Result<(), String> {
        let automation = uia_get()?;
        unsafe {
            let root: IUIAutomationElement = automation
                .ElementFromHandle(HWND(hwnd as *mut c_void))
                .map_err(|e| format!("ElementFromHandle: {e}"))?;

            let condition = automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_TabItemControlTypeId.0),
                )
                .map_err(|e| format!("CreatePropertyCondition: {e}"))?;

            let elements = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|e| format!("FindAll: {e}"))?;

            let count = elements.Length().unwrap_or(0);
            if index < 0 || index >= count {
                return Err(format!(
                    "tab index {index} out of range (browser has {count} tabs)"
                ));
            }

            let tab = elements
                .GetElement(index)
                .map_err(|e| format!("GetElement({index}): {e}"))?;

            // Get the SelectionItemPattern. UIA returns the pattern
            // as IUnknown; we cast to the typed pattern interface.
            let pattern_unknown = tab
                .GetCurrentPattern(UIA_SelectionItemPatternId)
                .map_err(|e| format!("GetCurrentPattern(SelectionItem): {e}"))?;
            let selection: IUIAutomationSelectionItemPattern = pattern_unknown
                .cast()
                .map_err(|e| format!("cast SelectionItemPattern: {e}"))?;

            selection.Select().map_err(|e| format!("Select: {e}"))?;

            // SelectionItem.Select() activates the tab in-process but
            // doesn't touch z-order / foreground. Bring the parent
            // window to front so the user sees the new active tab.
            // Reuses the same direct-Win32 approach from
            // actions::windows::focus_window — keeps the
            // foreground-rights handling consistent.
            crate::actions::windows::focus_window(hwnd)?;
        }
        Ok(())
    }

    /// Required so VARIANT::from(UIA_TabItemControlTypeId.0) compiles —
    /// `UIA_TabItemControlTypeId.0` is an i32 (the underlying control
    /// type id). VARIANT::from(i32) exists. Sanity-asserted here.
    #[allow(dead_code)]
    fn _assert_variant_from_i32() {
        let _ = VARIANT::from(UIA_TabItemControlTypeId.0);
        let _ = UIA_NamePropertyId; // silence dead_code if unused
    }
}

// --- macOS Accessibility (AX) implementation ---
//
// Browser tabs on macOS are exposed via the same Accessibility API
// the rest of the macOS switcher uses. The window-switcher side-table
// in `actions::windows::macos` already holds a retained
// `AXUIElementRef` for each surfaced browser window; we look up that
// ref via `lookup_window_ax` and walk its descendants for the tab
// strip.
//
// Tab-strip role conventions per browser:
//
// - **Safari** — tab strip is `AXTabGroup` directly; children are
//   `AXRadioButton` (counterintuitive but matches the AppKit segment
//   control they're built on). `AXPress` switches.
// - **Chromium-family** (Chrome / Brave / Edge / Vivaldi / Opera /
//   Arc) — tab strip is also `AXTabGroup` with `AXRadioButton`
//   children. Arc's sidebar is an `AXOutline` + `AXRow` instead;
//   we fall back to that role when no `AXTabGroup` is found.
// - **Firefox** — its a11y tree is lazy on macOS; first query may
//   return empty. The generic `AXTabGroup` walk below works once
//   the tree is populated.
//
// Performance: the AX tree walk crosses a process boundary per call.
// We mirror the Windows UIA cache (1s TTL keyed by hwnd) so repeated
// keystrokes on the search hot path don't repay the walk.

#[cfg(target_os = "macos")]
mod mac {
    use super::TabEntry;
    use crate::actions::windows::lookup_window_ax;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::os::raw::{c_int, c_void};
    use std::time::{Duration, Instant};

    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};

    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    type AXUIElementRef = *const c_void;
    type AXError = i32;
    const AX_OK: AXError = 0;

    const K_AX_ROLE_ATTRIBUTE: &str = "AXRole";
    const K_AX_CHILDREN_ATTRIBUTE: &str = "AXChildren";
    const K_AX_TITLE_ATTRIBUTE: &str = "AXTitle";
    const K_AX_DESCRIPTION_ATTRIBUTE: &str = "AXDescription";
    const K_AX_PRESS_ACTION: &str = "AXPress";

    // Roles we consider "tab containers". AXTabGroup is the standard
    // one (Safari / Chromium); AXOutline is Arc's sidebar fallback.
    const TAB_CONTAINER_ROLES: &[&str] = &["AXTabGroup", "AXOutline"];
    // Roles we consider "tabs" inside a container.
    const TAB_LEAF_ROLES: &[&str] = &["AXRadioButton", "AXTab", "AXRow"];

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementPerformAction(
            element: AXUIElementRef,
            action: CFStringRef,
        ) -> AXError;
    }

    thread_local! {
        /// 1-second TTL cache keyed by window hwnd. Same lifetime
        /// strategy as the Windows UIA module — repeated search
        /// keystrokes within a single user interaction reuse the
        /// previous walk's results.
        static TAB_CACHE: RefCell<HashMap<i64, (Instant, Vec<TabEntry>)>> =
            RefCell::new(HashMap::new());
    }
    const CACHE_TTL: Duration = Duration::from_secs(1);

    /// RAII for a +1 retained AX element. Drop releases.
    struct AXRef(AXUIElementRef);
    impl AXRef {
        unsafe fn take(p: AXUIElementRef) -> Option<Self> {
            if p.is_null() {
                None
            } else {
                Some(AXRef(p))
            }
        }
        fn as_raw(&self) -> AXUIElementRef {
            self.0
        }
    }
    impl Drop for AXRef {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0 as CFTypeRef) };
            }
        }
    }

    unsafe fn copy_attr(element: AXUIElementRef, attr: &str) -> Option<CFTypeRef> {
        let cf_attr = CFString::new(attr);
        let mut out: CFTypeRef = std::ptr::null();
        let err =
            AXUIElementCopyAttributeValue(element, cf_attr.as_concrete_TypeRef(), &mut out);
        if err != AX_OK || out.is_null() {
            None
        } else {
            Some(out)
        }
    }

    fn read_string_attr(element: AXUIElementRef, attr: &str) -> Option<String> {
        unsafe {
            let raw = copy_attr(element, attr)?;
            let cf_str = CFString::wrap_under_create_rule(raw as CFStringRef);
            Some(cf_str.to_string())
        }
    }

    /// Read the `AXChildren` array as a Vec of retained AXRefs. We
    /// retain each child since `CFArrayGetValueAtIndex` returns a
    /// borrowed reference and we want to outlive the array.
    fn read_children(element: AXUIElementRef) -> Vec<AXRef> {
        let raw = match unsafe { copy_attr(element, K_AX_CHILDREN_ATTRIBUTE) } {
            Some(p) => p,
            None => return Vec::new(),
        };
        let array: CFArray<CFTypeRef> =
            unsafe { CFArray::wrap_under_create_rule(raw as CFArrayRef) };
        let mut out = Vec::with_capacity(array.len() as usize);
        for i in 0..array.len() {
            if let Some(item_ref) = array.get(i) {
                let p = *item_ref;
                if !p.is_null() {
                    unsafe {
                        core_foundation::base::CFRetain(p);
                        if let Some(r) = AXRef::take(p as AXUIElementRef) {
                            out.push(r);
                        }
                    }
                }
            }
        }
        out
    }

    /// Depth-first search for a descendant whose AXRole matches one
    /// of `TAB_CONTAINER_ROLES`. Bounded depth (we walk at most 6
    /// levels) so a malformed tree can't loop us.
    fn find_tab_container(root: AXUIElementRef, depth: u8) -> Option<AXRef> {
        if depth > 6 {
            return None;
        }
        // Check root itself.
        if let Some(role) = read_string_attr(root, K_AX_ROLE_ATTRIBUTE) {
            if TAB_CONTAINER_ROLES.contains(&role.as_str()) {
                unsafe {
                    core_foundation::base::CFRetain(root as CFTypeRef);
                    return AXRef::take(root);
                }
            }
        }
        // Recurse into children.
        for child in read_children(root) {
            if let Some(found) = find_tab_container(child.as_raw(), depth + 1) {
                return Some(found);
            }
        }
        None
    }

    /// Read the visible label of a tab. Tabs commonly carry their
    /// page title in `AXTitle`; some builds (Arc rows, certain
    /// Chromium beta channels) put it in `AXDescription` instead.
    /// Try both.
    fn read_tab_label(tab: AXUIElementRef) -> Option<String> {
        if let Some(t) = read_string_attr(tab, K_AX_TITLE_ATTRIBUTE) {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(t) = read_string_attr(tab, K_AX_DESCRIPTION_ATTRIBUTE) {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        None
    }

    pub fn get_browser_tabs(hwnd: i64, process_name: &str) -> Result<Vec<TabEntry>, String> {
        // Cache check.
        let now = Instant::now();
        let cached = TAB_CACHE.with(|c| {
            let map = c.borrow();
            map.get(&hwnd)
                .filter(|(t, _)| now.duration_since(*t) < CACHE_TTL)
                .map(|(_, tabs)| tabs.clone())
        });
        if let Some(tabs) = cached {
            return Ok(tabs);
        }

        let handle = match lookup_window_ax(hwnd) {
            Some(h) => h,
            None => return Ok(Vec::new()), // Window not in current snapshot.
        };
        let window = handle.raw() as AXUIElementRef;

        let container = match find_tab_container(window, 0) {
            Some(c) => c,
            None => {
                // No tab strip found — probably a non-browser window
                // with a process name in the BROWSER list (e.g., a
                // helper). Cache empty so we don't re-walk the tree.
                TAB_CACHE.with(|c| {
                    c.borrow_mut().insert(hwnd, (now, Vec::new()));
                });
                return Ok(Vec::new());
            }
        };

        let mut tabs = Vec::new();
        for (idx, child) in read_children(container.as_raw()).into_iter().enumerate() {
            // Filter children by role: only tab leaves count. Some
            // tab strips include separators / "+" buttons that we
            // don't want to surface.
            let role = match read_string_attr(child.as_raw(), K_AX_ROLE_ATTRIBUTE) {
                Some(r) => r,
                None => continue,
            };
            if !TAB_LEAF_ROLES.contains(&role.as_str()) {
                continue;
            }
            let label = match read_tab_label(child.as_raw()) {
                Some(l) => l,
                None => continue,
            };
            tabs.push(TabEntry {
                window_hwnd: hwnd,
                process_name: process_name.to_string(),
                name: label,
                index: idx as i32,
            });
        }

        TAB_CACHE.with(|c| {
            c.borrow_mut().insert(hwnd, (now, tabs.clone()));
        });
        Ok(tabs)
    }

    pub fn focus_browser_tab(hwnd: i64, index: i32) -> Result<(), String> {
        let handle = lookup_window_ax(hwnd)
            .ok_or_else(|| format!("window {hwnd} not in snapshot — re-search"))?;
        let window = handle.raw() as AXUIElementRef;
        let pid = handle.pid;

        let container = find_tab_container(window, 0)
            .ok_or_else(|| String::from("no tab strip found in this window"))?;

        // Walk the same children, filter by role, count to `index`.
        // Re-walking is safer than caching AX refs across IPC: a few
        // ms of extra work on activation is fine, but a stale ref
        // would crash.
        let children = read_children(container.as_raw());
        let mut leaf_idx = 0;
        for child in children {
            let role = match read_string_attr(child.as_raw(), K_AX_ROLE_ATTRIBUTE) {
                Some(r) => r,
                None => continue,
            };
            if !TAB_LEAF_ROLES.contains(&role.as_str()) {
                continue;
            }
            if leaf_idx == index {
                // Press the tab.
                unsafe {
                    let action = CFString::new(K_AX_PRESS_ACTION);
                    let err = AXUIElementPerformAction(
                        child.as_raw(),
                        action.as_concrete_TypeRef(),
                    );
                    if err != AX_OK {
                        return Err(format!("AX press failed (err {err})"));
                    }
                }
                // Bring the browser to the foreground (cross-Space
                // activation). Failure is non-fatal.
                unsafe {
                    let cls = objc2::class!(NSRunningApplication);
                    let app: *mut AnyObject =
                        msg_send![cls, runningApplicationWithProcessIdentifier: pid as c_int];
                    if !app.is_null() {
                        let _: () = msg_send![app, activateWithOptions: 1u64];
                    }
                }
                return Ok(());
            }
            leaf_idx += 1;
        }

        Err(format!("tab index {index} out of range"))
    }
}

// --- Linux AT-SPI implementation ---
//
// AT-SPI is the Linux a11y framework — a D-Bus protocol implemented
// by the at-spi2-core daemon. Browsers expose tab strips as
// `PageTabList` accessibles with `PageTab` children.
//
// Reality of per-browser support:
//
// - **Firefox**: a11y is enabled by default in modern builds; the
//   tree is populated lazily on first AT-SPI query and may be empty
//   for ~100ms after launch. Generally reliable.
// - **Chromium-family** (Chrome, Brave, Edge, Vivaldi, Opera, Arc):
//   the AT-SPI bridge is gated behind `--force-renderer-accessibility`
//   or the presence of an active screen reader (Orca). Without one of
//   these, the renderer's a11y tree is empty even though the browser
//   window's outer chrome (tab strip included) IS exposed. So tab
//   enumeration usually works on Chromium; per-page content
//   inspection wouldn't.
//
// We probe each browser process once per session and cache the
// result. If the AT-SPI tree has no PageTabList for a given window,
// subsequent calls short-circuit to empty without re-walking.
//
// Sync API: zbus's `blocking-api` feature gives us a synchronous
// proxy. The search hot path is sync and we don't want to drag a
// tokio runtime into actions code, so we make all D-Bus calls
// blocking. Each call is local (Unix socket); typical round-trip
// is sub-millisecond, so even a deep tree walk is fast enough for
// the 1s TTL cache to amortize.

#[cfg(target_os = "linux")]
mod linux_atspi {
    use super::TabEntry;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    use zbus::blocking::connection::Builder as ConnectionBuilder;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::OwnedObjectPath;

    const ATSPI_BUS: &str = "org.a11y.Bus";
    const ATSPI_BUS_PATH: &str = "/org/a11y/bus";
    const ATSPI_BUS_IFACE: &str = "org.a11y.Bus";

    const ATSPI_REGISTRY_BUS: &str = "org.a11y.atspi.Registry";
    const ATSPI_REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
    const ATSPI_REGISTRY_IFACE: &str = "org.a11y.atspi.Registry";

    const ACCESSIBLE_IFACE: &str = "org.a11y.atspi.Accessible";
    const ACTION_IFACE: &str = "org.a11y.atspi.Action";

    /// AT-SPI Role enum (ABI-stable since AT-SPI 2.0). Sourced from
    /// `Atspi.Role` in the GTK docs. We only need the two we filter on.
    const ROLE_PAGE_TAB: u32 = 36;
    const ROLE_PAGE_TAB_LIST: u32 = 37;

    /// Bound the tree walk so a malformed accessible tree can't
    /// loop us forever. Browser tab strips are at depth 2-4 from the
    /// frame root in practice.
    const MAX_DEPTH: u8 = 8;

    thread_local! {
        /// Per-thread connection caches. AT-SPI client connections
        /// are cheap to keep open; reusing one across calls avoids
        /// the bus-discovery dance on every search keystroke.
        static SESSION_BUS: RefCell<Option<Connection>> = const { RefCell::new(None) };
        static A11Y_BUS: RefCell<Option<Connection>> = const { RefCell::new(None) };

        /// 1s TTL cache keyed by hwnd, mirroring the Windows UIA
        /// and macOS AX modules.
        static TAB_CACHE: RefCell<HashMap<i64, (Instant, Vec<TabEntry>)>> =
            RefCell::new(HashMap::new());

        /// Per-PID "AT-SPI exposes a tab strip for this process"
        /// flag. Populated on first successful enumeration; once a
        /// PID is marked unsupported (e.g. the renderer-only Chromium
        /// case where no PageTabList shows up) we short-circuit
        /// subsequent calls to avoid re-walking the tree.
        static PID_SUPPORT: RefCell<HashMap<u32, bool>> =
            RefCell::new(HashMap::new());
    }
    const CACHE_TTL: Duration = Duration::from_secs(1);

    fn session_bus() -> Result<Connection, String> {
        SESSION_BUS.with(|cell| {
            if let Some(c) = cell.borrow().as_ref() {
                return Ok(c.clone());
            }
            let conn = Connection::session().map_err(|e| format!("session bus: {e}"))?;
            *cell.borrow_mut() = Some(conn.clone());
            Ok(conn)
        })
    }

    /// Discover and connect to the AT-SPI accessibility bus. The
    /// session bus's `org.a11y.Bus.GetAddress` returns the address
    /// of a separate D-Bus daemon dedicated to a11y traffic.
    fn a11y_bus() -> Result<Connection, String> {
        A11Y_BUS.with(|cell| {
            if let Some(c) = cell.borrow().as_ref() {
                return Ok(c.clone());
            }
            let session = session_bus()?;
            let proxy = Proxy::new(&session, ATSPI_BUS, ATSPI_BUS_PATH, ATSPI_BUS_IFACE)
                .map_err(|e| format!("a11y proxy: {e}"))?;
            let address: String = proxy
                .call("GetAddress", &())
                .map_err(|e| format!("GetAddress: {e}"))?;
            let conn = ConnectionBuilder::address(address.as_str())
                .map_err(|e| format!("a11y address parse: {e}"))?
                .build()
                .map_err(|e| format!("a11y bus connect ({address}): {e}"))?;
            *cell.borrow_mut() = Some(conn.clone());
            Ok(conn)
        })
    }

    /// Resolve the unix PID of a unique D-Bus name (e.g. `:1.123`)
    /// via the standard `org.freedesktop.DBus.GetConnectionUnixProcessID`
    /// RPC on the session bus.
    fn pid_of_dbus_name(name: &str) -> Option<u32> {
        let session = session_bus().ok()?;
        let proxy = Proxy::new(
            &session,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .ok()?;
        proxy.call("GetConnectionUnixProcessID", &name).ok()
    }

    /// Look up the X11 `_NET_WM_PID` for an X11 window id. We re-do
    /// this per call rather than caching since the window switcher's
    /// own `WindowEntry.pid` isn't plumbed through to browser_tabs.
    /// One round-trip to the X server, ~sub-millisecond.
    fn pid_of_window(hwnd: i64) -> Result<u32, String> {
        if hwnd <= 0 || hwnd > u32::MAX as i64 {
            return Err(format!("invalid X11 window id: {hwnd}"));
        }
        use x11rb::connection::Connection as XConnection;
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
        let (conn, _) = x11rb::connect(None).map_err(|e| format!("X11 connect: {e}"))?;
        let pid_atom = conn
            .intern_atom(false, b"_NET_WM_PID")
            .map_err(|e| format!("intern: {e}"))?
            .reply()
            .map_err(|e| format!("intern reply: {e}"))?
            .atom;
        let reply = conn
            .get_property(false, hwnd as u32, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .map_err(|e| format!("get_property: {e}"))?
            .reply()
            .map_err(|e| format!("get_property reply: {e}"))?;
        if reply.format != 32 || reply.value.len() < 4 {
            return Err(format!("window {hwnd:#x} has no _NET_WM_PID"));
        }
        let bytes = [reply.value[0], reply.value[1], reply.value[2], reply.value[3]];
        Ok(u32::from_ne_bytes(bytes))
    }

    /// Resolve the AT-SPI Application root accessible (bus_name +
    /// object_path) for a given OS PID by enumerating registry
    /// applications and matching their owning unix pid.
    fn find_app_root(pid: u32) -> Result<(String, OwnedObjectPath), String> {
        let conn = a11y_bus()?;
        let registry = Proxy::new(
            &conn,
            ATSPI_REGISTRY_BUS,
            ATSPI_REGISTRY_PATH,
            ATSPI_REGISTRY_IFACE,
        )
        .map_err(|e| format!("registry proxy: {e}"))?;
        let apps: Vec<(String, OwnedObjectPath)> = registry
            .call("GetApplications", &())
            .map_err(|e| format!("GetApplications: {e}"))?;
        for (bus_name, path) in apps {
            if let Some(p) = pid_of_dbus_name(&bus_name) {
                if p == pid {
                    return Ok((bus_name, path));
                }
            }
        }
        Err(format!("no AT-SPI application registered for pid {pid}"))
    }

    /// Read an Accessible's role (u32) and child count (i32). One
    /// round-trip per property.
    fn role_and_child_count(
        conn: &Connection,
        bus_name: &str,
        path: &OwnedObjectPath,
    ) -> Option<(u32, i32)> {
        let proxy = Proxy::new(conn, bus_name, path.as_str(), ACCESSIBLE_IFACE).ok()?;
        let role: u32 = proxy.call("GetRole", &()).ok()?;
        let count: i32 = proxy.get_property("ChildCount").ok()?;
        Some((role, count))
    }

    fn get_child(
        conn: &Connection,
        bus_name: &str,
        path: &OwnedObjectPath,
        index: i32,
    ) -> Option<(String, OwnedObjectPath)> {
        let proxy = Proxy::new(conn, bus_name, path.as_str(), ACCESSIBLE_IFACE).ok()?;
        proxy.call("GetChildAtIndex", &index).ok()
    }

    fn get_name(conn: &Connection, bus_name: &str, path: &OwnedObjectPath) -> Option<String> {
        let proxy = Proxy::new(conn, bus_name, path.as_str(), ACCESSIBLE_IFACE).ok()?;
        proxy.get_property::<String>("Name").ok()
    }

    /// DFS for a descendant accessible whose role is `PageTabList`.
    fn find_tab_list(
        conn: &Connection,
        bus_name: &str,
        path: OwnedObjectPath,
        depth: u8,
    ) -> Option<(String, OwnedObjectPath)> {
        if depth > MAX_DEPTH {
            return None;
        }
        let (role, child_count) = role_and_child_count(conn, bus_name, &path)?;
        if role == ROLE_PAGE_TAB_LIST {
            return Some((bus_name.to_string(), path));
        }
        for i in 0..child_count {
            if let Some((cb, cp)) = get_child(conn, bus_name, &path, i) {
                if let Some(found) = find_tab_list(conn, &cb, cp, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }

    pub fn get_browser_tabs(hwnd: i64, process_name: &str) -> Result<Vec<TabEntry>, String> {
        let now = Instant::now();
        let cached = TAB_CACHE.with(|c| {
            let map = c.borrow();
            map.get(&hwnd)
                .filter(|(t, _)| now.duration_since(*t) < CACHE_TTL)
                .map(|(_, tabs)| tabs.clone())
        });
        if let Some(tabs) = cached {
            return Ok(tabs);
        }

        let pid = pid_of_window(hwnd)?;

        // Per-pid support cache: short-circuit for processes we
        // already know don't expose tabs over AT-SPI (Chromium
        // without renderer-accessibility, etc.).
        let prev_support = PID_SUPPORT.with(|c| c.borrow().get(&pid).copied());
        if let Some(false) = prev_support {
            return Ok(Vec::new());
        }

        let (bus_name, app_path) = match find_app_root(pid) {
            Ok(v) => v,
            Err(_) => {
                PID_SUPPORT.with(|c| {
                    c.borrow_mut().insert(pid, false);
                });
                TAB_CACHE.with(|c| {
                    c.borrow_mut().insert(hwnd, (now, Vec::new()));
                });
                return Ok(Vec::new());
            }
        };

        let conn = a11y_bus()?;
        let tab_list = match find_tab_list(&conn, &bus_name, app_path, 0) {
            Some(v) => v,
            None => {
                // App is registered with AT-SPI but exposes no tab
                // strip — common for Chromium without the
                // accessibility flag. Mark unsupported so we don't
                // re-walk the tree on every keystroke.
                PID_SUPPORT.with(|c| {
                    c.borrow_mut().insert(pid, false);
                });
                TAB_CACHE.with(|c| {
                    c.borrow_mut().insert(hwnd, (now, Vec::new()));
                });
                return Ok(Vec::new());
            }
        };

        // Mark supported on the first successful tab-list discovery.
        PID_SUPPORT.with(|c| {
            c.borrow_mut().insert(pid, true);
        });

        // Enumerate tabs.
        let (_, list_count) = match role_and_child_count(&conn, &tab_list.0, &tab_list.1) {
            Some(v) => v,
            None => return Ok(Vec::new()),
        };
        let mut tabs = Vec::with_capacity(list_count.max(0) as usize);
        let mut leaf_idx = 0_i32;
        for i in 0..list_count {
            let (cb, cp) = match get_child(&conn, &tab_list.0, &tab_list.1, i) {
                Some(v) => v,
                None => continue,
            };
            // Filter children: only include ones with role PageTab.
            // Some browsers add separators / "+" buttons.
            let role = role_and_child_count(&conn, &cb, &cp).map(|(r, _)| r);
            if role != Some(ROLE_PAGE_TAB) {
                continue;
            }
            let name = match get_name(&conn, &cb, &cp) {
                Some(n) if !n.trim().is_empty() => n,
                _ => continue,
            };
            tabs.push(TabEntry {
                window_hwnd: hwnd,
                process_name: process_name.to_string(),
                name,
                index: leaf_idx,
            });
            leaf_idx += 1;
        }

        TAB_CACHE.with(|c| {
            c.borrow_mut().insert(hwnd, (now, tabs.clone()));
        });
        Ok(tabs)
    }

    pub fn focus_browser_tab(hwnd: i64, index: i32) -> Result<(), String> {
        let pid = pid_of_window(hwnd)?;
        let (bus_name, app_path) = find_app_root(pid)?;
        let conn = a11y_bus()?;
        let tab_list = find_tab_list(&conn, &bus_name, app_path, 0)
            .ok_or_else(|| String::from("no tab strip found in this window"))?;

        // Re-walk to find the Nth PageTab leaf — same filter as
        // enumeration so the index matches.
        let (_, list_count) = role_and_child_count(&conn, &tab_list.0, &tab_list.1)
            .ok_or_else(|| String::from("tab list inaccessible"))?;
        let mut leaf_idx = 0_i32;
        for i in 0..list_count {
            let (cb, cp) = match get_child(&conn, &tab_list.0, &tab_list.1, i) {
                Some(v) => v,
                None => continue,
            };
            let role = role_and_child_count(&conn, &cb, &cp).map(|(r, _)| r);
            if role != Some(ROLE_PAGE_TAB) {
                continue;
            }
            if leaf_idx == index {
                // AT-SPI Action interface: DoAction(0) is "click" for
                // most accessibles; for tabs it equates to selection.
                let action = Proxy::new(&conn, cb.as_str(), cp.as_str(), ACTION_IFACE)
                    .map_err(|e| format!("action proxy: {e}"))?;
                let _: bool = action
                    .call("DoAction", &0_i32)
                    .map_err(|e| format!("DoAction: {e}"))?;

                // Bring the X11 window to the foreground so the
                // browser is visible (the AT-SPI selection alone
                // doesn't focus the window).
                use crate::actions::windows;
                let _ = windows::focus_window(hwnd);
                return Ok(());
            }
            leaf_idx += 1;
        }

        Err(format!("tab index {index} out of range"))
    }
}
