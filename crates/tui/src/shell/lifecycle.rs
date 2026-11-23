use editor::{EditMode, SaveCompletion};

use crate::{
    buffer,
    data_object::{CommandId, DeferredAction, Model, RunningState, StatusMessage},
    document,
    manager::effect::Effect,
};

/// Application lifecycle commands and save-completion coordination.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "quit" => request_quit(model),
        "close_buffer" => request_close(model),
        "close_other_buffers" => request_close_group(model, CloseGroup::Others),
        "close_buffers_right" => request_close_group(model, CloseGroup::Right),
        "close_all_buffers" => request_close_group(model, CloseGroup::All),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown lifecycle command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

pub(crate) enum SaveOutcome {
    Saved(editor::BufferId),
    Failed(String),
}

pub(crate) fn complete_save(model: &mut Model, completion: SaveCompletion) -> SaveOutcome {
    let buffer_id = completion.buffer_id();
    match model.editor.complete_save(completion) {
        Ok(_) => SaveOutcome::Saved(buffer_id),
        Err(error) => SaveOutcome::Failed(error.to_string()),
    }
}

pub(crate) fn save_failed(model: &mut Model, error: &str) {
    model.deferred_action = None;
    model.status = Some(StatusMessage::error(error));
}

pub(crate) fn continue_save(model: &mut Model, buffer_id: editor::BufferId) -> Option<Effect> {
    match model.deferred_action {
        Some(DeferredAction::Quit) => finish_quit(model),
        Some(DeferredAction::CloseBuffer(id)) => finish_close(model, id),
        Some(DeferredAction::CloseOthers(id)) => finish_close_group(model, id, CloseGroup::Others),
        Some(DeferredAction::CloseRight(id)) => finish_close_group(model, id, CloseGroup::Right),
        Some(DeferredAction::CloseAll(id)) => finish_close_group(model, id, CloseGroup::All),
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
            let path = model
                .editor
                .buffer_path(id)
                .map(std::path::Path::to_path_buf);
            match model.editor.close_buffer(id) {
                Ok(()) => {
                    model.status = None;
                    path.and_then(|path| document::close_file(model, path))
                }
                Err(error) => {
                    model.status = Some(StatusMessage::error(error.to_string()));
                    None
                }
            }
        }
        None => {
            model.deferred_action = None;
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseGroup {
    Others,
    Right,
    All,
}

impl CloseGroup {
    fn deferred(self, active: editor::BufferId) -> DeferredAction {
        match self {
            Self::Others => DeferredAction::CloseOthers(active),
            Self::Right => DeferredAction::CloseRight(active),
            Self::All => DeferredAction::CloseAll(active),
        }
    }
}

fn close_group_targets(
    model: &Model,
    active: editor::BufferId,
    group: CloseGroup,
) -> Vec<editor::BufferId> {
    let Some(snapshot) = model.editor.active_snapshot() else {
        return Vec::new();
    };
    match group {
        CloseGroup::All => snapshot
            .buffers
            .into_iter()
            .map(|buffer| buffer.id)
            .collect(),
        CloseGroup::Others => snapshot
            .buffers
            .into_iter()
            .filter(|buffer| buffer.id != active)
            .map(|buffer| buffer.id)
            .collect(),
        CloseGroup::Right => snapshot
            .buffers
            .iter()
            .position(|buffer| buffer.id == active)
            .map(|position| {
                snapshot
                    .buffers
                    .into_iter()
                    .skip(position + 1)
                    .map(|buffer| buffer.id)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn finish_close_group(
    model: &mut Model,
    active: editor::BufferId,
    group: CloseGroup,
) -> Option<Effect> {
    let targets = close_group_targets(model, active, group);
    if targets.is_empty() {
        model.deferred_action = None;
        model.status = None;
        return None;
    }
    if targets.iter().any(|&id| model.editor.is_saving(id)) {
        return None;
    }
    if let Some(id) = targets
        .iter()
        .copied()
        .find(|&id| model.editor.is_modified(id) == Some(true))
    {
        model.status = Some(StatusMessage::info("Saving before closing buffers"));
        return buffer::start_save(model, id);
    }

    model.deferred_action = None;
    let paths = targets
        .iter()
        .filter_map(|&id| {
            model
                .editor
                .buffer_path(id)
                .map(std::path::Path::to_path_buf)
        })
        .collect::<Vec<_>>();
    match model.editor.close_buffers(&targets) {
        Ok(()) => {
            model.status = None;
            crate::welcome::sync_context(model);
            close_documents(model, paths)
        }
        Err(error) => {
            model.status = Some(StatusMessage::error(error.to_string()));
            None
        }
    }
}

fn close_documents(model: &mut Model, paths: Vec<std::path::PathBuf>) -> Option<Effect> {
    paths.into_iter().fold(None, |effect, path| {
        Effect::combine(effect, document::close_file(model, path))
    })
}

fn follow_up_save(model: &mut Model, id: editor::BufferId) -> Option<Effect> {
    match model.editor.mode() {
        EditMode::Insert => None,
        EditMode::Normal | EditMode::Visual | EditMode::VisualLine => {
            buffer::save_if_dirty(model, id)
        }
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

fn request_close_group(model: &mut Model, group: CloseGroup) -> Option<Effect> {
    model.editor.enter_normal_mode();
    let Some(active) = model.editor.active_buffer() else {
        model.status = Some(StatusMessage::info("No file open"));
        return None;
    };
    model.deferred_action = Some(group.deferred(active));
    finish_close_group(model, active, group)
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
        Ok(()) => {
            model.status = None;
            crate::welcome::sync_context(model);
        }
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
        test_utils::{create_buffer, open_file},
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        let Some(Effect::Save(request)) = effect else {
            panic!("expected a save effect");
        };
        request
    }

    #[test]
    fn close_right_uses_tab_order_and_close_others_keeps_active() {
        let mut model = Model::default();
        let first = create_buffer(&mut model, "first");
        let second = create_buffer(&mut model, "second");
        let third = create_buffer(&mut model, "third");
        let fourth = create_buffer(&mut model, "fourth");
        model.editor.activate_buffer(third).unwrap();
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_buffers_right")),
        );
        assert_eq!(model.editor.buffer_count(), 3);
        assert_eq!(model.editor.active_buffer(), Some(third));
        assert!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .buffers
                .iter()
                .any(|buffer| buffer.id == first)
        );
        assert!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .buffers
                .iter()
                .any(|buffer| buffer.id == second)
        );
        assert!(
            !model
                .editor
                .active_snapshot()
                .unwrap()
                .buffers
                .iter()
                .any(|buffer| buffer.id == fourth)
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_other_buffers")),
        );
        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.editor.active_buffer(), Some(third));
    }

    #[tokio::test]
    async fn close_others_saves_a_dirty_target_before_closing() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = Model::default();
        let first = open_file(&mut model, &first_path);
        let second = open_file(&mut model, &second_path);
        model.editor.activate_buffer(first).unwrap();
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();
        model.editor.enter_normal_mode();
        model.editor.activate_buffer(second).unwrap();
        let manager = MessageManager::default();

        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_other_buffers")),
        ));
        assert_eq!(
            model.deferred_action,
            Some(DeferredAction::CloseOthers(second))
        );
        assert_eq!(model.editor.buffer_count(), 2);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.editor.active_buffer(), Some(second));
        assert_eq!(std::fs::read_to_string(first_path).unwrap(), "!first");
    }

    #[test]
    fn close_all_leaves_the_empty_editor_state() {
        let mut model = Model::default();
        create_buffer(&mut model, "first");
        create_buffer(&mut model, "second");
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_all_buffers")),
        );

        assert!(model.editor.is_empty());
        assert_eq!(model.editor.active_buffer(), None);
        assert!(model.editor.active_snapshot().is_none());
        assert_eq!(model.deferred_action, None);
    }

    #[tokio::test]
    async fn close_all_saves_a_dirty_active_buffer_before_closing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "original").unwrap();
        let mut model = Model::default();
        let id = open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();
        model.editor.enter_normal_mode();
        let manager = MessageManager::default();

        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_all_buffers")),
        ));
        assert_eq!(model.deferred_action, Some(DeferredAction::CloseAll(id)));
        assert_eq!(model.editor.buffer_count(), 1);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert!(model.editor.is_empty());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "!original");
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
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("b")
            .unwrap();
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
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("b")
            .unwrap();
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
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("local ")
            .unwrap();
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
        manager.update(&mut model, Message::TextInput('x'));
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
