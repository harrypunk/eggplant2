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

    switch(model, movement)
}

pub(crate) fn switch(model: &mut Model, movement: BufferMovement) -> Option<Effect> {
    let effect = auto_save_active(model);
    match movement {
        BufferMovement::Next => {
            let _ = model.editor.activate_next_buffer();
        }
        BufferMovement::Previous => {
            let _ = model.editor.activate_previous_buffer();
        }
    }
    let reveal = model
        .editor
        .active_buffer()
        .and_then(|id| reveal_buffer(model, id));
    Effect::combine(
        effect,
        Effect::combine(reveal, crate::vcs::sync_active_buffer(model)),
    )
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

pub(crate) fn reveal_buffer(model: &mut Model, id: BufferId) -> Option<Effect> {
    let path = model
        .editor
        .buffer_path(id)
        .map(std::path::Path::to_path_buf)?;
    let root = model.workspace.as_ref()?.root().to_path_buf();
    model
        .explorer
        .reveal_request(&root, &path)
        .map(Effect::feature)
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

    fn apply(model: &mut Model, effect: Effect) {
        match effect {
            Effect::Feature(request) => {
                MessageManager::default().update(model, request.perform());
            }
            Effect::Batch(effects) => {
                for effect in effects {
                    apply(model, effect);
                }
            }
            _ => {}
        }
    }

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        match effect {
            Some(Effect::Save(request)) => request,
            Some(Effect::Batch(effects)) => effects
                .into_iter()
                .map(|effect| expect_save(Some(effect)))
                .next()
                .expect("expected a save effect"),
            _ => panic!("expected a save effect"),
        }
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
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();
        model.editor.enter_normal_mode();
        let second = open_file(&mut model, &second_path);
        model.editor.activate_buffer(first).unwrap();
        let manager = MessageManager::default();

        let request = expect_save(
            manager.update(&mut model, Message::InvokeCommand(CommandId("next_buffer"))),
        );
        assert_eq!(model.editor.active_buffer(), Some(second));
        let completion = request.perform().await;
        manager.update(&mut model, Message::completion(completion));

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

        manager.update(&mut model, Message::CommandMenuStart);
        for key in ['b', 'n'] {
            let effect = manager.update(
                &mut model,
                Message::CommandMenuKey(KeyCode::Char(key).into()),
            );
            if let Some(effect) = effect {
                apply(&mut model, effect);
            }
        }
        assert_eq!(model.editor.active_buffer(), Some(first));
        assert_eq!(
            model
                .explorer
                .selected_entry()
                .map(|entry| entry.path.as_path()),
            Some(std::fs::canonicalize(&first_path).unwrap().as_path())
        );

        manager.update(&mut model, Message::CommandMenuStart);
        for key in ['b', 'p'] {
            let effect = manager.update(
                &mut model,
                Message::CommandMenuKey(KeyCode::Char(key).into()),
            );
            if let Some(effect) = effect {
                apply(&mut model, effect);
            }
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

        manager.update(&mut model, Message::CommandMenuStart);
        for key in ['b', 'n'] {
            manager.update(
                &mut model,
                Message::CommandMenuKey(KeyCode::Char(key).into()),
            );
        }

        assert_eq!(model.editor.active_buffer(), Some(first));
        assert!(!model.command_menu.is_active());
    }
}
