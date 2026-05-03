//! Handler for `SearchAction::RunCommand`.
//!
//! Thin delegation to the existing `execute_command` helper which
//! looks the command id up in the static registry and runs its
//! platform-specific implementation (sleep, lock, restart, etc.).
//! No state side effects beyond what the command itself does.

use crate::actions::system::execute_command;

pub fn handle(command: String) -> Result<(), String> {
    execute_command(&command)
}
