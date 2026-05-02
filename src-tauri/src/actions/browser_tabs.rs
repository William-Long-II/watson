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

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn get_browser_tabs(_hwnd: i64, _process_name: &str) -> Result<Vec<TabEntry>, String> {
    // Linux has AT-SPI but browser support is patchy and gated
    // behind per-browser flags / screen-reader presence — tracked
    // as a separate issue. Until that lands, return empty so the
    // search path stays well-behaved (window-only switcher rows
    // still work; tabs simply don't surface).
    Ok(vec![])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
