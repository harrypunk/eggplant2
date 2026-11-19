//! Fixed-root filesystem explorer state and traversal.

mod add;
mod buffers;
mod commands;
mod delete;
mod fs;
mod help;
mod state;
mod transfer;
mod work;

pub(crate) use add::overlay as add_overlay;
pub use commands::handle_command;
pub(crate) use commands::refresh_changed;
pub(crate) use delete::overlay as delete_overlay;
pub(crate) use help::overlay as help_overlay;
pub(crate) use state::TreeChange;
pub use state::{ExpansionState, ExplorerEntry, ExplorerEntryKind, ExplorerState, IgnoreMode};
pub(crate) use transfer::overlay as transfer_overlay;

pub fn register_text_input(
    input: &mut crate::manager::text_input_registry::TextInputRegistry,
) -> color_eyre::Result<()> {
    add::register_text_input(input)?;
    transfer::register_text_input(input)
}
