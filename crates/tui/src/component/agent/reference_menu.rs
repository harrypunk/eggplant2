use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget},
};

use crate::{agent::FileReferenceStatus, component::picker, theme::UiTheme};

const MAX_VISIBLE_REFERENCES: u16 = 8;

#[derive(Clone)]
pub struct AgentReferenceProps {
    entries: Vec<String>,
    selected: Option<usize>,
    status: FileReferenceStatus,
}

impl AgentReferenceProps {
    pub fn new(entries: Vec<String>, selected: Option<usize>, status: FileReferenceStatus) -> Self {
        Self {
            entries,
            selected,
            status,
        }
    }
}

pub struct AgentReferenceMenu;

impl AgentReferenceMenu {
    pub fn height(references: Option<&AgentReferenceProps>) -> u16 {
        references.map_or(0, |references| {
            u16::try_from(references.entries.len().max(1))
                .unwrap_or(u16::MAX)
                .min(MAX_VISIBLE_REFERENCES)
        })
    }

    pub fn render(
        references: &AgentReferenceProps,
        styles: &UiTheme,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let lines = if references.entries.is_empty() {
            vec![Line::styled(empty_message(references.status), styles.muted)]
        } else {
            references
                .entries
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let style = if references.selected == Some(index) {
                        styles.base.patch(styles.selection)
                    } else {
                        styles.popup_surface()
                    };
                    Line::styled(path.clone(), style)
                })
                .collect()
        };
        let lines = picker::visible_lines(lines, references.selected, area.height);
        Paragraph::new(lines)
            .style(styles.popup_surface())
            .render(area, buffer);
    }
}

fn empty_message(status: FileReferenceStatus) -> &'static str {
    match status {
        FileReferenceStatus::Loading | FileReferenceStatus::Filtering => "Searching files…",
        FileReferenceStatus::Ready => "No matching files",
        FileReferenceStatus::Failed => "File search unavailable",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_bounded_paths_and_typed_empty_states() {
        let theme = Theme::default();
        let entries = (0..12).map(|index| format!("file{index}.rs")).collect();
        let references = AgentReferenceProps::new(entries, Some(11), FileReferenceStatus::Ready);
        assert_eq!(AgentReferenceMenu::height(Some(&references)), 8);

        let mut terminal = Terminal::new(TestBackend::new(32, 3)).unwrap();
        terminal
            .draw(|frame| {
                AgentReferenceMenu::render(
                    &references,
                    theme.ui(),
                    frame.area(),
                    frame.buffer_mut(),
                )
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("file11.rs"));
        assert!(!content.contains("file0.rs"));

        for (status, message) in [
            (FileReferenceStatus::Loading, "Searching files…"),
            (FileReferenceStatus::Ready, "No matching files"),
            (FileReferenceStatus::Failed, "File search unavailable"),
        ] {
            let empty = AgentReferenceProps::new(Vec::new(), None, status);
            assert_eq!(AgentReferenceMenu::height(Some(&empty)), 1);
            assert_eq!(empty_message(status), message);
        }
    }
}
