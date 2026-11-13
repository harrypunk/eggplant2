//! Fixed-root filesystem explorer state and traversal.

mod commands;
mod fs;
mod state;

pub use commands::handle_command;
pub use state::{ExpansionState, ExplorerEntry, ExplorerEntryKind, ExplorerState, IgnoreMode};
