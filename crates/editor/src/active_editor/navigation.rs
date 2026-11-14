use helix_core::{
    RopeSlice,
    doc_formatter::TextFormat,
    graphemes,
    movement::{self, Direction, Movement},
    text_annotations::TextAnnotations,
};
use helix_stdx::rope::RopeSliceExt;

use crate::session::{
    CursorMovement, DocumentPosition, EditorError, LineBoundary, VerticalDirection, WordMovement,
};

use super::ActiveEditor;

type MoveFn = fn(
    RopeSlice,
    helix_core::Range,
    Direction,
    usize,
    Movement,
    &TextFormat,
    &mut TextAnnotations,
) -> helix_core::Range;

impl ActiveEditor<'_> {
    /// Move the active cursor without extending the selection.
    pub fn move_cursor(&mut self, movement: CursorMovement) -> Result<(), EditorError> {
        self.move_cursor_by(movement, 1)
    }

    /// Move by Helix word boundaries without extending the selection.
    pub fn move_cursor_word(&mut self, movement: WordMovement) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let selection = self
            .document
            .selection(self.view.id)
            .clone()
            .transform(|range| {
                let moved = match movement {
                    WordMovement::NextStart => movement::move_next_word_start(text, range, 1),
                    WordMovement::PreviousStart => movement::move_prev_word_start(text, range, 1),
                    WordMovement::NextEnd => movement::move_next_word_end(text, range, 1),
                };
                let target = match movement {
                    WordMovement::NextEnd => moved.cursor(text),
                    WordMovement::NextStart | WordMovement::PreviousStart => moved.head,
                };
                self.put_cursor(text, range, target)
            });
        self.document.set_selection(self.view.id, selection);
        self.ensure_cursor_in_view();
        Ok(())
    }

    /// Move to the current line's start or final grapheme without changing text.
    pub fn move_cursor_to_line_boundary(
        &mut self,
        boundary: LineBoundary,
    ) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let line_start = text.line_to_char(line);
        let line_end = helix_core::line_ending::line_end_char_index(&text, line);
        let target = match boundary {
            LineBoundary::Start => line_start,
            LineBoundary::End if line_end == line_start => line_start,
            LineBoundary::End => graphemes::prev_grapheme_boundary(text, line_end),
        };
        let selection = self
            .document
            .selection(self.view.id)
            .clone()
            .transform(|range| self.put_cursor(text, range, target));
        self.document.set_selection(self.view.id, selection);
        self.ensure_cursor_in_view();
        Ok(())
    }

    /// Move the active cursor to a logical document position without extending the selection.
    pub fn move_cursor_to(&mut self, position: DocumentPosition) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let line = position.line.min(text.len_lines().saturating_sub(1));
        let line_start = text.line_to_char(line);
        let line_end = helix_core::line_ending::line_end_char_index(&text.slice(..), line);
        let target = line_start + position.character.min(line_end.saturating_sub(line_start));
        let selection = self
            .document
            .selection(self.view.id)
            .clone()
            .transform(|range| self.put_cursor(text, range, target));
        self.document.set_selection(self.view.id, selection);
        self.ensure_cursor_in_view();
        Ok(())
    }

    /// Move to a zero-based line's first non-blank character, clamping at the last line.
    pub fn move_cursor_to_line(&mut self, line: usize) -> Result<(), EditorError> {
        let line = line.min(self.document.text().len_lines().saturating_sub(1));
        let character = self
            .document
            .text()
            .line(line)
            .first_non_whitespace_char()
            .unwrap_or(0);
        self.move_cursor_to(DocumentPosition { line, character })
    }

    /// Move vertically by a feature-computed number of logical lines.
    pub fn move_cursor_lines(
        &mut self,
        direction: VerticalDirection,
        lines: usize,
    ) -> Result<(), EditorError> {
        self.move_cursor_by(direction.into(), lines)
    }

    fn move_cursor_by(
        &mut self,
        movement: CursorMovement,
        count: usize,
    ) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let text_format = self
            .document
            .text_format(self.view.inner_area(self.document).width, None);
        let mut annotations = self.view.text_annotations(self.document, None);
        let movement_behavior = self.movement();

        let selection = self
            .document
            .selection(self.view.id)
            .clone()
            .transform(|range| {
                let (direction, move_fn): (Direction, MoveFn) = match movement {
                    CursorMovement::Left => (Direction::Backward, movement::move_horizontally),
                    CursorMovement::Right => (Direction::Forward, movement::move_horizontally),
                    CursorMovement::Up => (Direction::Backward, movement::move_vertically),
                    CursorMovement::Down => (Direction::Forward, movement::move_vertically),
                };
                move_fn(
                    text,
                    range,
                    direction,
                    count,
                    movement_behavior,
                    &text_format,
                    &mut annotations,
                )
            });
        drop(annotations);

        self.document.set_selection(self.view.id, selection);
        self.ensure_cursor_in_view();
        Ok(())
    }
}
