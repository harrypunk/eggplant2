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

    /// Delete the grapheme under the primary cursor.
    pub fn delete_grapheme_forward(&mut self) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        self.delete_range(
            cursor,
            graphemes::next_grapheme_boundary(text, cursor),
            cursor,
        );
        self.commit_history();
        Ok(())
    }

    /// Delete the primary characterwise selection and leave the cursor at its start.
    pub fn delete_selection(&mut self) -> Result<(), EditorError> {
        let range = self.document.selection(self.view.id).primary();
        self.delete_range(range.from(), range.to(), range.from());
        self.commit_history();
        Ok(())
    }

    /// Delete the primary selection for a following Insert session.
    pub fn change_selection(&mut self) -> Result<(), EditorError> {
        let range = self.document.selection(self.view.id).primary();
        self.delete_range(range.from(), range.to(), range.from());
        Ok(())
    }

    /// Delete from the cursor to the next word start.
    pub fn delete_word(&mut self) -> Result<(), EditorError> {
        let (start, end) = self.word_range();
        self.delete_range(start, end, start);
        self.commit_history();
        Ok(())
    }

    /// Delete from the cursor to the next word start for a following Insert session.
    pub fn change_word(&mut self) -> Result<(), EditorError> {
        let (start, end) = self.word_range();
        self.delete_range(start, end, start);
        Ok(())
    }

    /// Delete the current line, including its line-ending relationship to neighbors.
    pub fn delete_current_line(&mut self) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let line_start = text.line_to_char(line);
        let line_end = helix_core::line_ending::line_end_char_index(&text, line);
        let (start, end) = if line + 1 < text.len_lines() {
            (line_start, text.line_to_char(line + 1))
        } else if line > 0 {
            (line_start - self.document.line_ending.len_chars(), line_end)
        } else {
            (line_start, line_end)
        };
        self.delete_range(start, end, start);
        self.commit_history();
        Ok(())
    }

    /// Clear the current line for change while preserving existing indentation.
    pub fn change_current_line(&mut self) -> Result<(), EditorError> {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let line_start = text.line_to_char(line);
        let line_end = helix_core::line_ending::line_end_char_index(&text, line);
        let content_start = text
            .line(line)
            .first_non_whitespace_char()
            .map_or(line_start, |character| line_start + character);
        self.delete_range(content_start, line_end, content_start);
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

    fn delete_range(&mut self, start: usize, end: usize, cursor: usize) {
        if start == end {
            return;
        }
        let transaction = Transaction::delete(self.document.text(), [(start, end)].into_iter())
            .with_selection(Selection::point(cursor));
        self.apply_transaction(transaction);
    }

    fn commit_history(&mut self) {
        self.document.append_changes_to_history(self.view);
    }

    fn apply_transaction(&mut self, transaction: Transaction) {
        self.document.apply(&transaction, self.view.id);
        self.ensure_cursor_in_view();
    }
}
