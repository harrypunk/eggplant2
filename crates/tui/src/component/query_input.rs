use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    widgets::Widget,
};

use super::text_input::TextInput;
use crate::theme::UiTheme;

/// Reusable case-insensitive substring query for picker-style filtering.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query {
    text: String,
}

impl Query {
    pub fn push(&mut self, character: char) {
        self.text.push(character);
    }

    pub fn pop(&mut self) {
        self.text.pop();
    }

    pub fn clear(&mut self) {
        self.text.clear();
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn matches(&self, candidate: &str) -> bool {
        let query = self.text.to_lowercase();
        candidate.to_lowercase().contains(&query)
    }
}

/// Cursor-aware query field composed from the shared single-line text input.
#[derive(Debug, Clone, Copy)]
pub struct QueryInput<'a> {
    query: &'a str,
    styles: &'a UiTheme,
}

impl<'a> QueryInput<'a> {
    pub fn new(query: &'a str, styles: &'a UiTheme) -> Self {
        Self { query, styles }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        TextInput::new(self.query, self.styles).cursor_position(area)
    }
}

impl Widget for QueryInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        TextInput::new(self.query, self.styles).render(area, buf);
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
