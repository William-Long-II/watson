//! Handler for `SearchAction::PasteSnippet`.
//!
//! Two-step expansion: (1) put the expansion on the clipboard so it
//! is useful even if step 2 fails; (2) synthesize Ctrl/Cmd+V into
//! the prior-focused window. The frontend hides Watson before the
//! IPC call so the OS focus has already returned to whatever the
//! user was typing in.
//!
//! Platform paths:
//! - Windows: Win32 `SendInput` with VK_CONTROL + VK_V virtual keys.
//!   Replaces a prior PowerShell SendKeys shell-out that
//!   intermittently toggled NumLock under foreground-change races
//!   (KB179987-style behavior).
//! - macOS: Quartz `CGEventCreateKeyboardEvent` + `CGEventPost` for
//!   Cmd+V at the session-level tap. Uses the Accessibility grant
//!   the switcher already requires; avoids prompting for the
//!   separate Automation permission `osascript` would have needed.
//! - Linux: `xdotool key ctrl+v` best-effort. If xdotool isn't
//!   installed the snippet is already on the clipboard so the user
//!   can paste manually with Ctrl+V.
//!
//! All three platforms drop the cold-start cost of spawning a
//! scripting host (~100-150ms per paste) compared to their previous
//! shell-out implementations.

use crate::clipboard::ClipboardManager;

pub fn handle(expansion: String, clipboard: &ClipboardManager) -> Result<(), String> {
    // Step 1: copy to clipboard. Always do this, even if step 2
    // fails — the user can paste manually as a fallback.
    clipboard.copy_to_clipboard(&expansion)?;
    // Step 2: OS paste.
    paste_via_os()
}

#[cfg(target_os = "windows")]
fn paste_via_os() -> Result<(), String> {
    use std::thread::sleep;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_CONTROL, VK_V,
    };

    // Give the previously-focused window a moment to fully regain
    // focus after Watson hides. Without this, the synthesized
    // Ctrl+V can race the activation and land on the desktop or on
    // Watson itself (now hidden).
    sleep(Duration::from_millis(80));

    fn key(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    // Down-down-up-up so Ctrl is held while V is struck.
    let inputs = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];

    let n = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if n != inputs.len() as u32 {
        return Err(format!(
            "SendInput accepted only {n} of {} keystrokes",
            inputs.len()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn paste_via_os() -> Result<(), String> {
    use std::os::raw::c_void;
    use std::thread::sleep;
    use std::time::Duration;

    type CGEventRef = *mut c_void;
    type CGEventSourceRef = *mut c_void;
    type CGEventTapLocation = u32;
    type CGEventFlags = u64;
    type CGKeyCode = u16;
    type CFTypeRef = *const c_void;

    /// Quartz session-level tap: posts after WindowServer processes
    /// events but before they reach apps in the user's session. The
    /// right level for "I'm a userland tool synthesizing input."
    const K_CG_SESSION_EVENT_TAP: CGEventTapLocation = 1;
    /// Modifier flag mask for Command. Apple's `CGEventFlags`
    /// bits — `kCGEventFlagMaskCommand`.
    const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x100000;
    /// HIToolbox `kVK_ANSI_V` — the V key's layout-independent
    /// virtual position. Cmd+V is the universal paste shortcut so
    /// the physical position is what matters, not the user's
    /// keyboard layout.
    const K_VK_ANSI_V: CGKeyCode = 0x09;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
        fn CGEventPost(tap: CGEventTapLocation, event: CGEventRef);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
    }

    // Same 80ms grace period as the Windows path: the previously-
    // focused window needs a beat to fully regain focus after
    // Watson hides, otherwise the synthesized keystrokes can land
    // on Watson itself or on the desktop.
    sleep(Duration::from_millis(80));

    unsafe fn post_v_event(key_down: bool) -> Result<(), String> {
        let event = CGEventCreateKeyboardEvent(std::ptr::null_mut(), K_VK_ANSI_V, key_down);
        if event.is_null() {
            return Err(String::from(
                "CGEventCreateKeyboardEvent returned null — Accessibility \
                 permission probably not granted",
            ));
        }
        CGEventSetFlags(event, K_CG_EVENT_FLAG_MASK_COMMAND);
        CGEventPost(K_CG_SESSION_EVENT_TAP, event);
        CFRelease(event as CFTypeRef);
        Ok(())
    }

    unsafe {
        post_v_event(true)?; // Cmd+V down
        post_v_event(false)?; // Cmd+V up
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn paste_via_os() -> Result<(), String> {
    // Linux best-effort. xdotool is widely available but not
    // guaranteed; if it's missing, the snippet is already on the
    // clipboard so the user can paste manually (Ctrl+V).
    match std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", "ctrl+v"])
        .spawn()
    {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "Could not synthesize paste (xdotool not found: {e}). The snippet is on your clipboard — press Ctrl+V manually."
        )),
    }
}
