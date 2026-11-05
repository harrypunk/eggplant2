use ratatui::layout::Rect;

use crate::{
    data_object::{Context, Message, Model, RunningState},
    page::editor::editor_content_area,
};

#[derive(Debug, Default)]
pub struct MessageManager;

impl MessageManager {
    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Message> {
        match msg {
            Message::Quit => {
                model.running_state = RunningState::Done;
            }
            Message::EnterInsertMode => {
                if model.editor.active_buffer().is_some() {
                    model.context_stack.push(Context::EditorInput);
                    model.editor.enter_insert_mode();
                }
            }
            Message::ExitInsertMode => {
                model
                    .context_stack
                    .pop_if(|ctx| ctx == &Context::EditorInput);
                model.editor.enter_normal_mode();
            }
            Message::MoveCursor(movement) => {
                let _ = model.editor.move_cursor(movement);
            }
            Message::InsertChar(character) => {
                let _ = model.editor.insert_text(&character.to_string());
            }
            Message::InsertNewline => {
                let _ = model.editor.insert_text("\n");
            }
            Message::Backspace => {
                let _ = model.editor.backspace();
            }
            Message::Undo => {
                let _ = model.editor.undo();
            }
            Message::Redo => {
                let _ = model.editor.redo();
            }
            Message::Resize(width, height) => {
                let area = editor_content_area(Rect::new(0, 0, width, height));
                model.editor.resize_active_view(area.width, area.height);
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use editor::EditMode;

    use super::*;

    #[test]
    fn mode_messages_update_editor_session_and_context() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.editor.create_buffer("");

        manager.update(&mut model, Message::EnterInsertMode);
        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(model.context_stack.last(), Some(&Context::EditorInput));

        manager.update(&mut model, Message::ExitInsertMode);
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn insert_mode_requires_an_active_buffer() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        manager.update(&mut model, Message::EnterInsertMode);

        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn editing_messages_update_text_history_and_viewport() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.editor.create_buffer("a");
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(
            &mut model,
            Message::MoveCursor(editor::CursorMovement::Right),
        );
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('b'));
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "ab");

        manager.update(&mut model, Message::Backspace);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "a");
        manager.update(&mut model, Message::InsertChar('c'));
        manager.update(&mut model, Message::ExitInsertMode);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "ac");

        manager.update(&mut model, Message::Undo);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "a");
        manager.update(&mut model, Message::Redo);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "ac");
    }
}
