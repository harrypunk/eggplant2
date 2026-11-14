mod commands;
mod state;
mod work;

pub use commands::handle_command;
pub(crate) use commands::{close, complete_filter, complete_preview, complete_scan, insert_char};
pub use state::{FileCollectionState, FileEntry, FilePickerState, PreviewState, ScanDepth};
pub use work::FilePickerRequest;

pub(crate) const POPUP_MAX_HEIGHT: u16 = 36;
// Subtract the outer popup border and the preview's own border.
pub(crate) const PREVIEW_VIEWPORT_HEIGHT: u16 = POPUP_MAX_HEIGHT - 4;
