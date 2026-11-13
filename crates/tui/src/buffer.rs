use editor::{BufferId, EditMode};

use crate::{
    data_object::{CommandId, Model, StatusMessage},
    manager::effect::Effect,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferMovement {
    Next,
    Previous,
}

/// Buffer feature commands and shared save/reveal capabilities.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    let movement = match id.as_str() {
        "next_buffer" => BufferMovement::Next,
        "previous_buffer" => BufferMovement::Previous,
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown buffer command `{}`",
                id.as_str()
            )));
            return None;
        }
    };

    let effect = auto_save_active(model);
    match movement {
        BufferMovement::Next => {
            let _ = model.editor.activate_next_buffer();
        }
        BufferMovement::Previous => {
            let _ = model.editor.activate_previous_buffer();
        }
    }
    if let Some(id) = model.editor.active_buffer() {
        reveal_buffer(model, id);
    }
    effect
}

pub(crate) fn auto_save_active(model: &mut Model) -> Option<Effect> {
    if model.editor.mode() == EditMode::Insert {
        return None;
    }
    save_active(model)
}

pub(crate) fn save_active(model: &mut Model) -> Option<Effect> {
    let id = model.editor.active_buffer()?;
    save_if_dirty(model, id)
}

pub(crate) fn save_if_dirty(model: &mut Model, id: BufferId) -> Option<Effect> {
    if model.editor.is_modified(id) != Some(true) || model.editor.is_saving(id) {
        return None;
    }
    start_save(model, id)
}

pub(crate) fn start_save(model: &mut Model, id: BufferId) -> Option<Effect> {
    match model.editor.save_buffer(id) {
        Ok(request) => Some(Effect::Save(request)),
        Err(error) => {
            model.deferred_action = None;
            model.status = Some(StatusMessage::error(error.to_string()));
            None
        }
    }
}

pub(crate) fn reveal_buffer(model: &mut Model, id: BufferId) {
    let Some(path) = model
        .editor
        .buffer_path(id)
        .map(std::path::Path::to_path_buf)
    else {
        return;
    };
    let Some(workspace) = &model.workspace else {
        return;
    };
    let root = workspace.root().to_path_buf();
    if let Err(error) = model.explorer.reveal(&root, &path) {
        model.status = Some(StatusMessage::error(format!(
            "failed to update file explorer `{}`: {error}",
            path.display()
        )));
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;
    use editor::SaveRequest;

    use super::*;
    use crate::{
        data_object::Message,
        manager::message_manager::MessageManager,
        startup::initial_model,
        test_utils::{create_buffer, open_file},
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        let Some(Effect::Save(request)) = effect else {
            panic!("expected a save effect");
        };
        request
    }

    #[tokio::test]
    async fn switching_buffers_saves_the_outgoing_dirty_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = Model::default();
        let first = open_file(&mut model, &first_path);
        model.editor.enter_insert_mode();
        model.editor.insert_text("!").unwrap();
        model.editor.enter_normal_mode();
        let second = open_file(&mut model, &second_path);
        model.editor.activate_buffer(first).unwrap();
        let manager = MessageManager::default();

        let request = expect_save(
            manager.update(&mut model, Message::InvokeCommand(CommandId("next_buffer"))),
        );
        assert_eq!(model.editor.active_buffer(), Some(second));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(first_path).unwrap(), "!first");
        assert!(!model.editor.is_modified(first).unwrap());
    }

    #[test]
    fn buffer_switch_commands_reveal_the_active_file_in_the_explorer() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = nested.join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let first = open_file(&mut model, &first_path);
        let second = open_file(&mut model, &second_path);
        let manager = MessageManager::default();

        for key in [' ', 'b', 'n'] {
            manager.update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)));
        }
        assert_eq!(model.editor.active_buffer(), Some(first));
        assert_eq!(
            model
                .explorer
                .selected_entry()
                .map(|entry| entry.path.as_path()),
            Some(std::fs::canonicalize(&first_path).unwrap().as_path())
        );

        for key in [' ', 'b', 'p'] {
            manager.update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)));
        }
        assert_eq!(model.editor.active_buffer(), Some(second));
        assert_eq!(
            model
                .explorer
                .selected_entry()
                .map(|entry| entry.path.as_path()),
            Some(std::fs::canonicalize(&second_path).unwrap().as_path())
        );
    }

    #[test]
    fn leader_buffer_sequence_switches_buffer() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        let first = create_buffer(&mut model, "first");
        create_buffer(&mut model, "second");

        for key in [' ', 'b', 'n'] {
            manager.update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)));
        }

        assert_eq!(model.editor.active_buffer(), Some(first));
        assert!(!model.command_menu.is_active());
    }
}
