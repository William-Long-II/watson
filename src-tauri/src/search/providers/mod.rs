//! Concrete `ResultProvider` implementations.
//!
//! Each existing result kind in the search dispatcher migrates here
//! one at a time. The dispatcher in `lib.rs::search` calls each
//! provider in turn and collects the merged results.
//!
//! Migration order (smallest to largest, validating the trait shape
//! before tackling the more complex providers):
//! 1. `web_search` — pure config, no I/O, no DB
//! 2. system commands, calculator (sync, contained)
//! 3. apps, snippets (sync with state borrow)
//! 4. windows, tabs (cross-platform FFI surface)
//! 5. async providers (notes, file_search) — needs trait-async
//!    decision

pub mod apps;
pub mod browser_tabs;
pub mod calculator;
pub mod captures;
pub mod file_search;
pub mod notes;
pub mod snippets;
pub mod system_commands;
pub mod web_search;
pub mod windows;
