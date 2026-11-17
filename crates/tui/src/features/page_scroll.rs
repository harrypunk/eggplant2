use editor::{EditorSession, VerticalDirection};

use crate::{
    capabilities::page_scroll::{ScrollDirection, ScrollViewport},
    data_object::{CommandId, Model, StatusMessage},
    manager::effect::Effect,
};

impl ScrollViewport for EditorSession {
    fn viewport_height(&self) -> Option<usize> {
        self.active_view_height()
    }

    fn scroll_lines(&mut self, direction: ScrollDirection, rows: usize) {
        let direction = match direction {
            ScrollDirection::Down => VerticalDirection::Down,
            ScrollDirection::Up => VerticalDirection::Up,
        };
        if let Ok(mut editor) = self.active_editor() {
            let _ = editor.scroll_lines(direction, rows);
        }
    }
}

/// Compose page-scroll commands with the main editor's viewport capability.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    let direction = match id.as_str() {
        "page_scroll_down" => ScrollDirection::Down,
        "page_scroll_up" => ScrollDirection::Up,
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown page-scroll command `{}`",
                id.as_str()
            )));
            return None;
        }
    };
    model.page_scroll.scroll(&mut model.editor, direction);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        capabilities::page_scroll::PageScroll, data_object::Message,
        manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    #[test]
    fn configured_step_scrolls_down_and_up_without_an_effect() {
        let mut model = Model::default();
        let text = (0..100)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        create_buffer(&mut model, &text);
        model.page_scroll = PageScroll::from_step("25%").unwrap();
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 12));
        let height = model.editor.active_view_height().unwrap();
        let expected = height / 4;

        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(CommandId("page_scroll_down"))
                )
                .is_none()
        );
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.first_line, expected.max(1));
        assert!(snapshot.cursor.is_some());

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("page_scroll_up")),
        );
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.first_line, 0);
        assert!(snapshot.cursor.is_some());
    }

    #[test]
    fn empty_and_unsized_documents_are_safe() {
        let mut model = Model::default();
        create_buffer(&mut model, "");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(0, 0));

        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(CommandId("page_scroll_down"))
                )
                .is_none()
        );
        assert_eq!(model.editor.active_snapshot().unwrap().text.line_count, 1);
    }
}
