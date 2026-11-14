mod navigation;
mod viewport;

use helix_view::{Document, View};

/// Focused capability for operations on the active document and its view.
///
/// Helix values remain inside the editor crate; TUI features use semantic methods
/// without gaining buffer ownership or lifecycle access.
pub struct ActiveEditor<'a> {
    pub(crate) document: &'a mut Document,
    pub(crate) view: &'a mut View,
    pub(crate) scrolloff: usize,
}

impl<'a> ActiveEditor<'a> {
    pub(crate) fn new(document: &'a mut Document, view: &'a mut View, scrolloff: usize) -> Self {
        Self {
            document,
            view,
            scrolloff,
        }
    }

    pub(crate) fn ensure_cursor_in_view(&mut self) {
        self.view
            .ensure_cursor_in_view(self.document, self.scrolloff);
    }
}
