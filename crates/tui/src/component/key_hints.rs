use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::leader::LeaderState;

/// Contextual hints for the pending leader sequence.
///
/// This is not the future complete/searchable keymap reference. The component
/// owns presentation and intrinsic size; the page owns popup placement.
#[derive(Debug, Clone)]
pub struct KeyHints {
    title: String,
    lines: Vec<Line<'static>>,
}

impl KeyHints {
    pub fn new(leader: &LeaderState) -> Self {
        let lines = leader
            .items()
            .iter()
            .map(|item| {
                Line::from(vec![
                    Span::styled(
                        item.key.to_string(),
                        Style::new().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(item.description),
                ])
            })
            .collect();
        Self {
            title: leader.context_title(),
            lines,
        }
    }

    pub fn size(&self) -> Size {
        let content_width = self
            .lines
            .iter()
            .map(Line::width)
            .max()
            .unwrap_or(0)
            .max(Line::from(self.title.as_str()).width());
        let width = u16::try_from(content_width)
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let height = u16::try_from(self.lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        Size { width, height }
    }
}

impl Widget for KeyHints {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        Paragraph::new(self.lines)
            .block(Block::bordered().title(self.title))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;

    #[test]
    fn sizes_a_vertical_list_to_its_content_and_borders() {
        let mut leader = LeaderState::default();
        leader.handle_key(KeyCode::Char(' '));
        let hints = KeyHints::new(&leader);

        assert_eq!(hints.lines.len(), 3);
        assert_eq!(hints.size().height, 5);
        assert_eq!(hints.size().width, 16);
    }

    #[test]
    fn title_and_size_follow_the_active_context_stack() {
        let mut leader = LeaderState::default();
        leader.handle_key(KeyCode::Char(' '));
        leader.handle_key(KeyCode::Char('b'));
        let hints = KeyHints::new(&leader);

        assert_eq!(hints.title, "Leader > buffer");
        assert_eq!(hints.size().width, 19);

        let mut leader = LeaderState::default();
        leader.handle_key(KeyCode::Char(' '));
        leader.handle_key(KeyCode::Char('q'));
        let hints = KeyHints::new(&leader);

        assert_eq!(hints.title, "Leader > quit");
        assert_eq!(hints.size().width, 15);
    }
}
