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
    ExtendLines,
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
            SelectionBehavior::Extend | SelectionBehavior::ExtendLines => {
                helix_core::movement::Movement::Extend
            }
        }
    }

    pub(crate) fn put_cursor(&self, text: RopeSlice, range: Range, target: usize) -> Range {
        match self.selection_behavior {
            SelectionBehavior::Move => range.put_cursor(text, target, false),
            SelectionBehavior::Extend => range.put_cursor(text, target, true),
            SelectionBehavior::ExtendLines => self.put_line_cursor(text, range, target),
        }
    }

    fn put_line_cursor(&self, text: RopeSlice, range: Range, target: usize) -> Range {
        let anchor_line = text.char_to_line(range.anchor);
        let target_line = text.char_to_line(target);
        let anchor_start = text.line_to_char(anchor_line);
        let anchor_end = line_end_with_ending(text, anchor_line);
        match target_line >= anchor_line {
            true => Range::new(anchor_start, line_end_with_ending(text, target_line)),
            false => Range::new(text.line_to_char(target_line), anchor_end),
        }
    }
}

fn line_end_with_ending(text: RopeSlice, line: usize) -> usize {
    if line + 1 < text.len_lines() {
        text.line_to_char(line + 1)
    } else {
        helix_core::line_ending::line_end_char_index(&text, line)
    }
}
