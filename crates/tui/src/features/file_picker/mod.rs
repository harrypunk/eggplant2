mod commands;
mod state;
mod work;

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};

pub const KEYMAPS: &[ContextKeymap] = &[ContextKeymap::new("file_picker", Context::FilePicker)
    .with_capabilities(&[
        KeymapCapability::ListNavigation,
        KeymapCapability::ModalActions,
    ])];

pub use crate::file_search::{FileEntry, ScanDepth};
pub(crate) use commands::{close, handle_input};
pub use commands::{
    close_file_picker, general_action_subscriptions, handle_general_action, next, open,
    open_selected, previous,
};
pub use state::{FileCollectionState, FilePickerState, PreviewState};
pub use work::FilePickerRequest;

pub(crate) const POPUP_MAX_HEIGHT: u16 = 36;
// Subtract the outer popup border and the preview's own border.
pub(crate) const PREVIEW_VIEWPORT_HEIGHT: u16 = POPUP_MAX_HEIGHT - 4;
