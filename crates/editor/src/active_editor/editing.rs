use helix_core::{Selection, Transaction, graphemes, indent};
use helix_stdx::rope::RopeSliceExt;

use crate::{
    document::default_syntax_loader,
    session::{EditorError, InsertPosition, OpenLineDirection},
};

use super::ActiveEditor;

impl ActiveEditor<'_> {
    /// Position the cursor for an Insert-mode variant without changing text.
    pub fn move_cursor_to_insert(&mut self, position: InsertPosition) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let line_end = helix_core::line_ending::line_end_char_index(&text, line);
        let target = match position {
            InsertPosition::AfterCursor => {
                graphemes::next_grapheme_boundary(text, cursor).min(line_end)
            }
            InsertPosition::LineEnd => line_end,
            InsertPosition::FirstNonBlank => text
                .line(line)
                .first_non_whitespace_char()
                .map_or(text.line_to_char(line), |character| {
                    text.line_to_char(line) + character
                }),
        };
        self.document
            .set_selection(self.view.id, Selection::point(target));
        self.ensure_cursor_in_view();
        Ok(())
    }

    /// Open an indented line below or above the cursor through one transaction.
    pub fn open_line(&mut self, direction: OpenLineDirection) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let current_line = text.char_to_line(cursor);
        let new_line = match direction {
            OpenLineDirection::Below => current_line + 1,
            OpenLineDirection::Above => current_line,
        };
        let previous_line = new_line.saturating_sub(1);
        let insert_at = match new_line {
            0 => 0,
            _ => helix_core::line_ending::line_end_char_index(&text, previous_line),
        };
        let loader = default_syntax_loader();
        let loader = loader.load();
        let indent = indent::indent_for_newline(
            &loader,
            self.document.syntax(),
            &helix_core::syntax::config::IndentationHeuristic::default(),
            &self.document.indent_style,
            self.document.tab_width(),
            text,
            previous_line,
            insert_at,
            current_line,
        );
        let line_ending = self.document.line_ending.as_str();
        let inserted = match (direction, new_line) {
            (OpenLineDirection::Above, 0) => format!("{indent}{line_ending}"),
            _ => format!("{line_ending}{indent}"),
        };
        let indent_len = indent.chars().count();
        let cursor = match (direction, new_line) {
            (OpenLineDirection::Above, 0) => insert_at + indent_len,
            _ => insert_at + line_ending.chars().count() + indent_len,
        };
        let transaction = Transaction::change(
            self.document.text(),
            [(insert_at, insert_at, Some(inserted.into()))].into_iter(),
        )
        .with_selection(Selection::point(cursor));
        self.apply_transaction(transaction);
        Ok(())
    }

    /// Insert text at each active selection cursor.
    pub fn insert_text(&mut self, text: &str) -> Result<(), EditorError> {
        if text.is_empty() {
            return Ok(());
        }

        let transaction = {
            let document_text = self.document.text().slice(..);
            Transaction::change_by_and_with_selection(
                self.document.text(),
                self.document.selection(self.view.id),
                |range| {
                    let cursor = range.cursor(document_text);
                    ((cursor, cursor, Some(text.into())), None)
                },
            )
        };

        self.apply_transaction(transaction);
        Ok(())
    }

    /// Delete the grapheme before each active selection cursor.
    pub fn backspace(&mut self) -> Result<(), EditorError> {
        let transaction = {
            let document_text = self.document.text().slice(..);
            Transaction::delete_by_and_with_selection(
                self.document.text(),
                self.document.selection(self.view.id),
                |range| {
                    let cursor = range.cursor(document_text);
                    (
                        (
                            graphemes::prev_grapheme_boundary(document_text, cursor),
                            cursor,
                        ),
                        None,
                    )
                },
            )
        };

        self.apply_transaction(transaction);
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool, EditorError> {
        let changed = self.document.undo(self.view);
        self.ensure_cursor_in_view();
        Ok(changed)
    }

    pub fn redo(&mut self) -> Result<bool, EditorError> {
        let changed = self.document.redo(self.view);
        self.ensure_cursor_in_view();
        Ok(changed)
    }

    fn apply_transaction(&mut self, transaction: Transaction) {
        self.document.apply(&transaction, self.view.id);
        self.ensure_cursor_in_view();
    }
}
