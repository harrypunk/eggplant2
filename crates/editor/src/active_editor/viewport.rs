use helix_core::char_idx_at_visual_offset;
use helix_view::{Document, View, view::ViewPosition};

use crate::session::{EditorError, VerticalDirection, ViewAlignment};

use super::ActiveEditor;

impl ActiveEditor<'_> {
    /// Align the cursor line in the visual viewport without changing document state.
    pub fn align_cursor_line(&mut self, alignment: ViewAlignment) -> Result<(), EditorError> {
        let alignment = match alignment {
            ViewAlignment::Top => helix_view::Align::Top,
            ViewAlignment::Center => helix_view::Align::Center,
            ViewAlignment::Bottom => helix_view::Align::Bottom,
        };
        helix_view::align_view(self.document, self.view, alignment);
        Ok(())
    }

    /// Scroll the visual viewport immediately, moving the cursor only when needed
    /// to keep it in the visible area.
    pub fn scroll_lines(
        &mut self,
        direction: VerticalDirection,
        lines: usize,
    ) -> Result<(), EditorError> {
        let cursor = self
            .document
            .selection(self.view.id)
            .primary()
            .cursor(self.document.text().slice(..));

        let view_offset = shifted_view_offset(self.document, self.view, direction, lines);
        self.document.set_view_offset(self.view.id, view_offset);
        if let Some(head) = cursor_correction_after_scroll(
            self.document,
            self.view,
            view_offset,
            cursor,
            direction,
            self.scrolloff,
        ) {
            self.document
                .set_selection(self.view.id, helix_core::Selection::point(head));
        }
        Ok(())
    }
}

fn shifted_view_offset(
    document: &Document,
    view: &View,
    direction: VerticalDirection,
    lines: usize,
) -> ViewPosition {
    let mut view_offset = document.view_offset(view.id);
    let lines = isize::try_from(lines).unwrap_or(isize::MAX);
    let offset = match direction {
        VerticalDirection::Down => lines,
        VerticalDirection::Up => lines.saturating_neg(),
    };
    let row_offset = isize::try_from(view_offset.vertical_offset)
        .unwrap_or(isize::MAX)
        .saturating_add(offset);
    (view_offset.anchor, view_offset.vertical_offset) =
        char_at_visual_row(document, view, view_offset.anchor, row_offset);
    view_offset
}

fn cursor_correction_after_scroll(
    document: &Document,
    view: &View,
    view_offset: ViewPosition,
    cursor: usize,
    direction: VerticalDirection,
    scrolloff: usize,
) -> Option<usize> {
    let height = view.inner_height();
    let scrolloff = scrolloff.min(height.saturating_sub(1) / 2);
    let base_offset = isize::try_from(view_offset.vertical_offset).unwrap_or(isize::MAX);
    match direction {
        VerticalDirection::Down => {
            let row_offset =
                base_offset.saturating_add(isize::try_from(scrolloff).unwrap_or(isize::MAX));
            let (mut head, off) =
                char_at_visual_row(document, view, view_offset.anchor, row_offset);
            head += usize::from(off != 0);
            (head > cursor).then_some(head)
        }
        VerticalDirection::Up => {
            let last_visible_row = height.saturating_sub(scrolloff).saturating_sub(1);
            let row_offset =
                base_offset.saturating_add(isize::try_from(last_visible_row).unwrap_or(isize::MAX));
            let (head, _) = char_at_visual_row(document, view, view_offset.anchor, row_offset);
            (head < cursor).then_some(head)
        }
    }
}

fn char_at_visual_row(
    document: &Document,
    view: &View,
    anchor: usize,
    row_offset: isize,
) -> (usize, usize) {
    let text = document.text().slice(..);
    let text_format = document.text_format(view.inner_area(document).width, None);
    let annotations = view.text_annotations(document, None);
    char_idx_at_visual_offset(text, anchor, row_offset, 0, &text_format, &annotations)
}
