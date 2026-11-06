use editor::EditorSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Tabs, Widget},
};

/// Filename-only tabs for the buffers in an editor snapshot.
#[derive(Debug, Clone, Copy)]
pub struct BufferTabs<'a> {
    snapshot: &'a EditorSnapshot,
}

impl<'a> BufferTabs<'a> {
    pub fn new(snapshot: &'a EditorSnapshot) -> Self {
        Self { snapshot }
    }
}

impl Widget for BufferTabs<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let selected = self
            .snapshot
            .buffers
            .iter()
            .position(|buffer| buffer.id == self.snapshot.buffer_id);
        let titles = self.snapshot.buffers.iter().map(|buffer| {
            let modified = match buffer.modified {
                editor::DirtyState::Dirty => " *",
                editor::DirtyState::Clean => "",
            };
            format!("{}{modified}", buffer.name)
        });

        Tabs::new(titles)
            .select(selected)
            .divider("|")
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, EditorSession};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn shows_file_names_with_dirty_marker() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let factory = DocumentFactory::new();
        let mut session = EditorSession::new();
        session.insert_document(factory.open(&first_path).unwrap());
        session.enter_insert_mode();
        session.insert_text("!").unwrap();
        session.enter_normal_mode();
        session.insert_document(factory.open(&second_path).unwrap());
        let snapshot = session.active_snapshot().unwrap();
        let backend = TestBackend::new(50, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(BufferTabs::new(&snapshot), frame.area()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("first.txt *"));
        assert!(content.contains("second.txt"));
        assert!(!content.contains(directory.path().to_str().unwrap()));
    }
}
