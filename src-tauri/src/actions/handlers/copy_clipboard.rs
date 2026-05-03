//! Handler for `SearchAction::CopyClipboard`.
//!
//! Single-shot copy: the dispatcher hands a string, the
//! `ClipboardManager` writes it to the OS clipboard. No further
//! state mutation; clipboard *history* is updated by the manager
//! itself when the next monitor tick observes the new system value.

use crate::clipboard::ClipboardManager;

pub fn handle(content: String, clipboard: &ClipboardManager) -> Result<(), String> {
    clipboard.copy_to_clipboard(&content)
}
