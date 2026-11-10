use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use crate::{
    data_object::{FocusState, Workspace},
    explorer::{ExpansionState, ExplorerEntry, ExplorerEntryKind, ExplorerState},
    theme::UiTheme,
};

#[derive(Debug)]
pub struct FileExplorer<'a> {
    workspace: Option<&'a Workspace>,
    explorer: &'a ExplorerState,
    focus: FocusState,
    styles: &'a UiTheme,
}

impl<'a> FileExplorer<'a> {
    pub fn new(
        workspace: Option<&'a Workspace>,
        explorer: &'a ExplorerState,
        focus: FocusState,
        styles: &'a UiTheme,
    ) -> Self {
        Self {
            workspace,
            explorer,
            focus,
            styles,
        }
    }
}

impl FileExplorer<'_> {
    fn title(&self) -> String {
        self.workspace.map_or_else(
            || "File Explorer".to_string(),
            |workspace| {
                let root = workspace.root();
                root.file_name()
                    .unwrap_or(root.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            },
        )
    }

    fn border_type(&self) -> BorderType {
        match self.focus {
            FocusState::Focused => BorderType::Double,
            FocusState::Unfocused => BorderType::Plain,
        }
    }

    fn content_lines(&self) -> Vec<Line<'static>> {
        if self.workspace.is_none() {
            return vec![Line::from("No folder open")];
        }
        if self.explorer.entries.is_empty() {
            return vec![Line::from("Empty directory")];
        }

        self.explorer
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                Self::entry_line(self.explorer.selected == Some(index), entry, self.styles)
            })
            .collect()
    }

    fn entry_line(selected: bool, entry: &ExplorerEntry, styles: &UiTheme) -> Line<'static> {
        let marker = if selected { ">" } else { " " };
        let (directory_marker, suffix) = match entry.kind {
            ExplorerEntryKind::Directory if entry.expansion == ExpansionState::Expanded => {
                ("▾ ", "/")
            }
            ExplorerEntryKind::Directory => ("▸ ", "/"),
            ExplorerEntryKind::Link => ("", " (link)"),
            _ => ("", ""),
        };
        let indentation = "  ".repeat(entry.depth);
        let text = format!(
            "{marker} {indentation}{directory_marker}{}{suffix}",
            entry.name.to_string_lossy()
        );
        let style = match selected {
            true => styles.base.patch(styles.selection),
            false => styles.base,
        };
        Line::styled(text, style)
    }
}

impl Widget for FileExplorer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = match self.focus {
            FocusState::Focused => self.styles.focused_border,
            FocusState::Unfocused => self.styles.border,
        };
        let block = Block::bordered()
            .border_type(self.border_type())
            .border_style(self.styles.base.patch(border_style))
            .style(self.styles.base)
            .title(self.title());
        Paragraph::new(self.content_lines())
            .style(self.styles.base)
            .block(block)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn title_uses_only_the_workspace_folder_name() {
        let workspace = Workspace::new(PathBuf::from("/tmp/project"));
        let explorer = ExplorerState::default();
        let theme = Theme::default();
        let component = FileExplorer::new(
            Some(&workspace),
            &explorer,
            FocusState::Unfocused,
            theme.ui(),
        );
        assert_eq!(component.title(), "project");

        let root = Workspace::new(PathBuf::from("/"));
        let component =
            FileExplorer::new(Some(&root), &explorer, FocusState::Unfocused, theme.ui());
        assert_eq!(component.title(), "/");
    }

    #[test]
    fn renders_sorted_entries_with_selection() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("folder")).unwrap();
        std::fs::write(directory.path().join("folder/child.txt"), "child").unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();
        std::os::unix::fs::symlink(
            directory.path().join("note.txt"),
            directory.path().join("link.txt"),
        )
        .unwrap();
        let mut explorer = ExplorerState::load(directory.path()).unwrap();
        let workspace = Workspace::new(directory.path().to_path_buf());
        let theme = Theme::default();
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    FileExplorer::new(Some(&workspace), &explorer, FocusState::Focused, theme.ui()),
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
        assert!(content.contains("> ▸ folder/"));
        assert!(content.contains("  note.txt"));
        assert!(content.contains("  link.txt (link)"));
        assert!(content.contains("║"));

        explorer.unfold_selected(directory.path()).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    FileExplorer::new(Some(&workspace), &explorer, FocusState::Focused, theme.ui()),
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
        assert!(content.contains("> ▾ folder/"));
        assert!(content.contains("    child.txt"));
    }
}
