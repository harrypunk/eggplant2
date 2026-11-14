mod editing;
mod navigation;
mod selection;
mod viewport;

use helix_core::{Range, RopeSlice};
use helix_view::{Document, View};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionBehavior {
    Move,
    Extend,
}

/// Focused capability for operations on the active document and its view.
///
/// Helix values remain inside the editor crate; TUI features use semantic methods
/// without gaining buffer ownership or lifecycle access.
pub struct ActiveEditor<'a> {
    pub(crate) document: &'a mut Document,
    pub(crate) view: &'a mut View,
    pub(crate) scrolloff: usize,
    pub(crate) selection_behavior: SelectionBehavior,
}

impl<'a> ActiveEditor<'a> {
    pub(crate) fn new(
        document: &'a mut Document,
        view: &'a mut View,
        scrolloff: usize,
        selection_behavior: SelectionBehavior,
    ) -> Self {
        Self {
            document,
            view,
            scrolloff,
            selection_behavior,
        }
    }

    pub(crate) fn ensure_cursor_in_view(&mut self) {
        self.view
            .ensure_cursor_in_view(self.document, self.scrolloff);
    }

    pub(crate) fn movement(&self) -> helix_core::movement::Movement {
        match self.selection_behavior {
            SelectionBehavior::Move => helix_core::movement::Movement::Move,
            SelectionBehavior::Extend => helix_core::movement::Movement::Extend,
        }
    }

    pub(crate) fn put_cursor(&self, text: RopeSlice, range: Range, target: usize) -> Range {
        range.put_cursor(
            text,
            target,
            self.selection_behavior == SelectionBehavior::Extend,
        )
    }
}
