mod commands;
mod state;
mod work;

pub(crate) use commands::{close, handle_input};
pub use commands::{general_action_subscriptions, handle_command, handle_general_action};
pub use state::{FileCollectionState, FileEntry, FilePickerState, PreviewState, ScanDepth};
pub use work::FilePickerRequest;

pub(crate) const POPUP_MAX_HEIGHT: u16 = 36;
// Subtract the outer popup border and the preview's own border.
pub(crate) const PREVIEW_VIEWPORT_HEIGHT: u16 = POPUP_MAX_HEIGHT - 4;
