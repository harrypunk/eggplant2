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

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};

pub const KEYMAPS: &[ContextKeymap] = &[
    ContextKeymap::new("explorer", Context::Explorer)
        .with_commands(&[("open_selected", "enter")])
        .with_capabilities(&[KeymapCapability::LeaderMenu]),
    ContextKeymap::new("explorer_help", Context::ExplorerHelp)
        .with_commands(&[("close_explorer_help", "esc"), ("explorer_help", "?")]),
    ContextKeymap::new("explorer_add", Context::ExplorerAdd).with_commands(&[
        ("explorer_add_submit", "enter"),
        ("close_explorer_add", "esc"),
    ]),
    ContextKeymap::new("explorer_delete", Context::ExplorerDelete).with_commands(&[
        ("explorer_delete_confirm", "y"),
        ("close_explorer_delete", "esc"),
    ]),
    ContextKeymap::new("explorer_transfer", Context::ExplorerTransfer).with_commands(&[
        ("explorer_transfer_submit", "enter"),
        ("close_explorer_transfer", "esc"),
    ]),
];

pub(crate) use add::{handle_input as handle_add_input, overlay as add_overlay};
pub use commands::{
    cancel_add, cancel_delete, cancel_transfer, close_help, confirm_delete, fold, next, open_add,
    open_copy, open_delete, open_move, previous, refresh, submit_add, submit_transfer, toggle_help,
    toggle_ignored_files, unfold,
};
pub(crate) use commands::{open_selected, refresh_changed};
pub(crate) use delete::overlay as delete_overlay;
pub(crate) use help::overlay as help_overlay;
pub(crate) use state::TreeChange;
pub use state::{ExpansionState, ExplorerEntry, ExplorerEntryKind, ExplorerState, IgnoreMode};
pub(crate) use transfer::{handle_input as handle_transfer_input, overlay as transfer_overlay};
