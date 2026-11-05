use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use crate::{
    data_object::Workspace,
    explorer::{ExplorerEntryKind, ExplorerState},
};

#[derive(Debug)]
pub struct FileExplorer<'a> {
    workspace: Option<&'a Workspace>,
    explorer: &'a ExplorerState,
    is_focused: bool,
}

impl<'a> FileExplorer<'a> {
    pub fn new(
        workspace: Option<&'a Workspace>,
        explorer: &'a ExplorerState,
        is_focused: bool,
    ) -> Self {
        Self {
            workspace,
            explorer,
            is_focused,
        }
    }
}

impl Widget for FileExplorer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self.workspace.map_or_else(
            || "File Explorer".to_string(),
            |_| format!("File Explorer | {}", self.explorer.current_dir.display()),
        );
        let lines = if self.workspace.is_none() {
            vec![Line::from("No folder open")]
        } else if self.explorer.entries.is_empty() {
            vec![Line::from("Empty directory")]
        } else {
            self.explorer
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let marker = if self.explorer.selected == Some(index) {
                        ">"
                    } else {
                        " "
                    };
                    let suffix = if entry.kind == ExplorerEntryKind::Directory {
                        "/"
                    } else {
                        ""
                    };
                    let line = format!("{marker} {}{suffix}", entry.name.to_string_lossy());
                    let style = if self.explorer.selected == Some(index) {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    };
                    Line::styled(line, style)
                })
                .collect()
        };

        let border = if self.is_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        };
        Paragraph::new(lines)
            .block(Block::bordered().border_type(border).title(title))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_sorted_entries_with_selection() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("folder")).unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();
        let explorer = ExplorerState::load(directory.path()).unwrap();
        let workspace = Workspace::new(directory.path().to_path_buf());
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    FileExplorer::new(Some(&workspace), &explorer, true),
                    frame.area(),
                );
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("> folder/"));
        assert!(content.contains("  note.txt"));
        assert!(content.contains("║"));
    }
}
