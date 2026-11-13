use editor::{EditMode, SaveCompletion};

use crate::{
    buffer,
    data_object::{CommandId, DeferredAction, Model, RunningState, StatusMessage},
    manager::effect::Effect,
};

/// Application lifecycle commands and save-completion coordination.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "quit" => request_quit(model),
        "close_buffer" => request_close(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown lifecycle command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

pub(crate) fn finish_save(model: &mut Model, completion: SaveCompletion) -> Option<Effect> {
    let buffer_id = completion.buffer_id();
    if let Err(error) = model.editor.complete_save(completion) {
        model.deferred_action = None;
        model.status = Some(StatusMessage::error(error.to_string()));
        return None;
    }
    model.status = Some(StatusMessage::info("Saved"));

    match model.deferred_action {
        Some(DeferredAction::Quit) => finish_quit(model),
        Some(DeferredAction::CloseBuffer(id)) => finish_close(model, id),
        None => follow_up_save(model, buffer_id),
    }
}

fn finish_quit(model: &mut Model) -> Option<Effect> {
    if model.editor.has_modified_buffers() || model.editor.has_saving_buffers() {
        return save_next_dirty_buffer(model);
    }
    model.deferred_action = None;
    model.running_state = RunningState::Done;
    None
}

fn finish_close(model: &mut Model, id: editor::BufferId) -> Option<Effect> {
    if model.editor.is_saving(id) {
        return None;
    }
    match model.editor.is_modified(id) {
        Some(true) => buffer::start_save(model, id),
        Some(false) => {
            model.deferred_action = None;
            match model.editor.close_buffer(id) {
                Ok(()) => model.status = None,
                Err(error) => {
                    model.status = Some(StatusMessage::error(error.to_string()));
                }
            }
            None
        }
        None => {
            model.deferred_action = None;
            None
        }
    }
}

fn follow_up_save(model: &mut Model, id: editor::BufferId) -> Option<Effect> {
    match model.editor.mode() {
        EditMode::Insert => None,
        EditMode::Normal => buffer::save_if_dirty(model, id),
    }
}

fn save_next_dirty_buffer(model: &mut Model) -> Option<Effect> {
    model
        .editor
        .first_dirty_buffer()
        .filter(|&id| !model.editor.is_saving(id))
        .and_then(|id| buffer::start_save(model, id))
}

fn request_quit(model: &mut Model) -> Option<Effect> {
    model.editor.enter_normal_mode();
    if model.editor.has_modified_buffers() || model.editor.has_saving_buffers() {
        model.deferred_action = Some(DeferredAction::Quit);
        model.status = Some(StatusMessage::info("Saving before quit"));
        return save_next_dirty_buffer(model);
    }

    model.deferred_action = None;
    model.running_state = RunningState::Done;
    None
}

fn request_close(model: &mut Model) -> Option<Effect> {
    model.editor.enter_normal_mode();
    let Some(id) = model.editor.active_buffer() else {
        model.status = Some(StatusMessage::info("No file open"));
        return None;
    };

    if model.editor.is_modified(id) == Some(true) || model.editor.is_saving(id) {
        model.deferred_action = Some(DeferredAction::CloseBuffer(id));
        model.status = Some(StatusMessage::info("Saving before close"));
        if model.editor.is_saving(id) {
            return None;
        }
        return buffer::start_save(model, id);
    }

    model.deferred_action = None;
    match model.editor.close_buffer(id) {
        Ok(()) => model.status = None,
        Err(error) => model.status = Some(StatusMessage::error(error.to_string())),
    }
    None
}

#[cfg(test)]
mod tests {
    use editor::SaveRequest;

    use super::*;
    use crate::{
        data_object::{Context, Message, StatusKind},
        manager::message_manager::MessageManager,
        test_utils::open_file,
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        let Some(Effect::Save(request)) = effect else {
            panic!("expected a save effect");
        };
        request
    }

    #[tokio::test]
    async fn dirty_quit_saves_then_exits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "a").unwrap();
        let manager = MessageManager::default();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model.editor.insert_text("b").unwrap();
        model.editor.enter_normal_mode();

        let request =
            expect_save(manager.update(&mut model, Message::InvokeCommand(CommandId("quit"))));
        assert_eq!(model.deferred_action, Some(DeferredAction::Quit));
        assert_eq!(model.running_state, RunningState::Running);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        assert_eq!(model.running_state, RunningState::Done);
    }

    #[tokio::test]
    async fn dirty_close_saves_then_closes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "a").unwrap();
        let manager = MessageManager::default();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model.editor.insert_text("b").unwrap();
        model.editor.enter_normal_mode();

        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_buffer")),
        ));
        assert_eq!(model.editor.buffer_count(), 1);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        assert!(model.editor.is_empty());
    }

    #[tokio::test]
    async fn failed_file_save_cancels_close_and_preserves_the_dirty_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.txt");
        std::fs::write(&path, "original").unwrap();
        let mut model = Model::default();
        let id = open_file(&mut model, &path);
        model.context_stack.push(Context::AppNormal);
        model.editor.enter_insert_mode();
        model.editor.insert_text("local ").unwrap();
        model.editor.enter_normal_mode();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        let manager = MessageManager::default();

        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_buffer")),
        ));
        assert_eq!(model.deferred_action, Some(DeferredAction::CloseBuffer(id)));
        let completion = request.perform().await;
        assert!(
            manager
                .update(&mut model, Message::SaveFinished(completion))
                .is_none()
        );

        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.editor.active_buffer(), Some(id));
        assert!(model.editor.is_modified(id).unwrap());
        assert_eq!(model.deferred_action, None);
        assert_eq!(model.running_state, RunningState::Running);
        assert_eq!(model.status.as_ref().unwrap().kind, StatusKind::Error);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    }

    #[tokio::test]
    async fn quit_waits_for_save_completion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.context_stack.push(Context::AppNormal);
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::InsertChar('x'));
        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        ));

        manager.update(&mut model, Message::InvokeCommand(CommandId("quit")));
        assert_eq!(model.running_state, RunningState::Running);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        manager.update(&mut model, Message::InvokeCommand(CommandId("quit")));
        assert_eq!(model.running_state, RunningState::Done);
    }

    #[test]
    fn leader_q_q_quits() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        for key in [' ', 'q', 'q'] {
            manager.update(
                &mut model,
                Message::CommandMenuKey(crossterm::event::KeyCode::Char(key)),
            );
        }

        assert_eq!(model.running_state, RunningState::Done);
    }
}
