use helix_core::{Selection, graphemes};
use helix_stdx::rope::RopeSliceExt;

use super::{ActiveEditor, SelectionBehavior};

impl ActiveEditor<'_> {
    /// Start a characterwise selection at the current cursor.
    pub fn start_selection(&mut self) {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let end = graphemes::next_grapheme_boundary(text, cursor);
        self.document
            .set_selection(self.view.id, Selection::single(cursor, end));
    }

    /// Start a linewise selection covering the current line and any existing line ending.
    pub fn start_line_selection(&mut self) {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let start = text.line_to_char(line);
        let end = if line + 1 < text.len_lines() {
            text.line_to_char(line + 1)
        } else {
            helix_core::line_ending::line_end_char_index(&text, line)
        };
        self.document
            .set_selection(self.view.id, Selection::single(start, end));
    }

    /// Collapse the primary selection to its moving cursor edge.
    pub fn collapse_selection(&mut self) {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        self.document
            .set_selection(self.view.id, Selection::point(cursor));
    }

    /// Read the current primary selection as owned text.
    pub fn selected_text(&self) -> String {
        if self.selection_behavior == SelectionBehavior::Move {
            return String::new();
        }
        let range = self.document.selection(self.view.id).primary();
        self.document
            .text()
            .slice(range.from()..range.to())
            .to_string()
    }

    /// Read from the cursor to the next word start.
    pub fn current_word_text(&self) -> String {
        let (start, end) = self.word_range();
        self.document.text().slice(start..end).to_string()
    }

    pub(super) fn word_range(&self) -> (usize, usize) {
        let text = self.document.text().slice(..);
        let range = self.document.selection(self.view.id).primary();
        let cursor = range.cursor(text);
        let end = helix_core::movement::move_next_word_start(text, range, 1).head;
        (cursor, end)
    }

    /// Read the current line, including an existing line ending.
    pub fn current_line_text(&self) -> String {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let start = text.line_to_char(line);
        let end = if line + 1 < text.len_lines() {
            text.line_to_char(line + 1)
        } else {
            helix_core::line_ending::line_end_char_index(&text, line)
        };
        text.slice(start..end).to_string()
    }

    /// Read the grapheme under the primary cursor.
    pub fn cursor_grapheme_text(&self) -> String {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let end = graphemes::next_grapheme_boundary(text, cursor);
        text.slice(cursor..end).to_string()
    }

    /// Read the content that `cc` removes, excluding indentation it preserves.
    pub fn current_line_change_text(&self) -> String {
        let text = self.document.text().slice(..);
        let cursor = self.document.selection(self.view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let line_start = text.line_to_char(line);
        let line_end = helix_core::line_ending::line_end_char_index(&text, line);
        let content_start = text
            .line(line)
            .first_non_whitespace_char()
            .map_or(line_start, |character| line_start + character);
        text.slice(content_start..line_end).to_string()
    }
}
