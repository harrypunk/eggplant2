use helix_core::{Selection, graphemes};

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
}
