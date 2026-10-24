//! SQLite migrations and OS-managed network/credential adapters.
pub const INITIAL_SCHEMA: &str = include_str!("../migrations/0001_initial.sql");
pub mod database;
pub mod network;

/// No ambient current-directory fallback for production application state.
pub fn require_absolute_state_directory(path: &std::path::Path) -> Result<(), &'static str> {
    if !path.is_absolute() {
        return Err("Application state directory must be absolute");
    }
    Ok(())
}

pub mod desktop_activity;
pub mod vault;
