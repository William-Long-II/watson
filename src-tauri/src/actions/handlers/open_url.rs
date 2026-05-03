//! Handler for `SearchAction::OpenUrl`.
//!
//! Hands the URL to the OS via the existing `actions::open_url`
//! helper (which itself runs the `validate_target` shell-injection
//! filter and `open::that` to launch the system default browser).
//! No state side effects — opens are stateless. Stats for web
//! searches are not recorded today; if that becomes a feature it
//! lands here.

use crate::actions::open_url as os_open_url;

pub fn handle(url: String) -> Result<(), String> {
    os_open_url(&url)
}
