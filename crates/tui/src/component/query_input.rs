use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    widgets::Widget,
};

use super::text_input::{TextInput, TextInputChange, TextInputEvent};
use crate::theme::UiTheme;

/// Reusable case-insensitive substring query for picker-style filtering.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query {
    input: TextInput,
}

impl Query {
    pub fn push(&mut self, character: char) {
        self.input.insert(character);
    }

    pub fn pop(&mut self) {
        self.input.backspace();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    pub fn handle_event(&mut self, event: TextInputEvent) -> TextInputChange {
        self.input.handle_event(event)
    }

    pub fn as_str(&self) -> &str {
        self.input.value()
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    pub fn matches(&self, candidate: &str) -> bool {
        let query = self.input.value().to_lowercase();
        candidate.to_lowercase().contains(&query)
    }
}

/// Cursor-aware query field composed from the shared single-line text input.
#[derive(Debug, Clone, Copy)]
pub struct QueryInput<'a> {
    query: &'a Query,
    styles: &'a UiTheme,
}

impl<'a> QueryInput<'a> {
    pub fn new(query: &'a Query, styles: &'a UiTheme) -> Self {
        Self { query, styles }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        self.query.input.cursor_position(area, self.styles)
    }
}

impl Widget for QueryInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.query.input.view(self.styles).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_empty_by_default_and_matches_case_insensitively() {
        let mut query = Query::default();
        assert!(query.matches("anything"));
        for character in "ToKy".chars() {
            query.push(character);
        }
        assert!(query.matches("tokyo-night"));
        assert!(query.matches("TOKYO"));
        assert!(!query.matches("catppuccin"));

        query.pop();
        assert!(query.matches("tokyo"));
        query.clear();
        assert!(query.is_empty());
        assert!(query.matches("anything"));
    }
}
