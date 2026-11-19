//! Fixed-root filesystem explorer state and traversal.

mod add;
mod commands;
mod delete;
mod fs;
mod help;
mod state;
mod work;

pub(crate) use add::overlay as add_overlay;
pub use add::register_text_input;
pub use commands::handle_command;
pub(crate) use commands::refresh_changed;
pub(crate) use delete::overlay as delete_overlay;
pub(crate) use help::overlay as help_overlay;
pub use state::{ExpansionState, ExplorerEntry, ExplorerEntryKind, ExplorerState, IgnoreMode};
