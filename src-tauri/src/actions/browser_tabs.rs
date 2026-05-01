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
];

pub fn is_browser_process(process_name: &str) -> bool {
    let lower = process_name.to_ascii_lowercase();
    BROWSER_PROCESS_NAMES.iter().any(|b| lower.contains(b))
}

#[cfg(target_os = "windows")]
pub use win::{focus_browser_tab, get_browser_tabs};

#[cfg(not(target_os = "windows"))]
pub fn get_browser_tabs(_hwnd: i64, _process_name: &str) -> Result<Vec<TabEntry>, String> {
    // UIA is Windows-only. macOS would need NSAccessibility / AXUIElement;
    // Linux has AT-SPI but browser support is patchy. Both are follow-up
    // work — return empty so the search path stays well-behaved.
    Ok(vec![])
}

#[cfg(not(target_os = "windows"))]
pub fn focus_browser_tab(_hwnd: i64, _index: i32) -> Result<(), String> {
    Err("Browser tab switching is Windows-only in v1".to_string())
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
