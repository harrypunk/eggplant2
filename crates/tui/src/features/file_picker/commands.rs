use crate::{
    data_object::{CommandId, Context, FocusTarget, Model, StatusMessage},
    document,
    manager::effect::Effect,
};

/// File-picker command adapter.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "open_file_picker" => open(model),
        "close_file_picker" => {
            close(model);
            None
        }
        "file_picker_backspace" => {
            let root = model.workspace.as_ref()?.root().to_path_buf();
            model.file_picker.backspace(&root).map(Effect::feature)
        }
        "open_picker_file" => {
            let path = model.file_picker.selected_entry()?.path.clone();
            let documents = model.file_picker.documents().clone();
            let session = model.file_picker.session_revision();
            document::open_file_at_with_origin(
                &documents,
                model,
                &path,
                None,
                document::OpenOrigin::FilePicker { session },
            )
        }
        "file_picker_next" => model.file_picker.select_next().map(Effect::feature),
        "file_picker_previous" => model.file_picker.select_previous().map(Effect::feature),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown file-picker command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

pub(crate) fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    model
        .file_picker
        .insert(&root, character)
        .map(Effect::feature)
}

pub(crate) fn close(model: &mut Model) {
    model.file_picker.close();
    model.modal.close(Context::FilePicker);
    model
        .context_stack
        .pop_if(|context| context == &Context::FilePicker);
    model.current_focus = match model.context_stack.last() {
        Some(Context::Explorer) => FocusTarget::FileExplorer,
        _ => FocusTarget::Editor,
    };
}

fn open(model: &mut Model) -> Option<Effect> {
    let Some(workspace) = &model.workspace else {
        model.status = Some(StatusMessage::info("No folder open"));
        return None;
    };
    let root = workspace.root().to_path_buf();
    if !model.modal.open(Context::FilePicker) {
        return None;
    }
    if model.context_stack.last() != Some(&Context::FilePicker) {
        model.context_stack.push(Context::FilePicker);
    }
    model.current_focus = FocusTarget::FilePicker;
    model.file_picker.open(&root).map(Effect::feature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{Message, Visibility},
        explorer::ExpansionState,
        file_picker::PreviewState,
        manager::message_manager::MessageManager,
        startup::initial_model,
    };

    fn apply(model: &mut Model, manager: &MessageManager, effect: Effect) {
        match effect {
            Effect::Feature(request) => {
                manager.update(model, request.perform());
            }
            Effect::Batch(effects) => {
                for effect in effects {
                    apply(model, manager, effect);
                }
            }
            _ => {}
        }
    }

    fn complete_open(model: &mut Model, manager: &MessageManager, effect: Option<Effect>) {
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("expected an open-document effect");
        };
        let effect = manager.update(model, Message::FileOpened(request.perform()));
        if let Some(effect) = effect {
            apply(model, manager, effect);
        }
    }

    #[test]
    fn another_modal_prevents_picker_composition() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        assert!(model.modal.open(Context::BufferGrep));

        let effect = MessageManager::default().update(
            &mut model,
            Message::InvokeCommand(CommandId("open_file_picker")),
        );

        assert!(effect.is_none());
        assert_eq!(model.file_picker.visibility, Visibility::Hidden);
        assert!(model.modal.is_active(Context::BufferGrep));
        assert_ne!(model.current_focus, FocusTarget::FilePicker);
    }

    #[test]
    fn old_open_does_not_close_a_reopened_picker_session() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_file_picker")),
        );
        let completion = match manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_picker_file")),
        ) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_file_picker")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_file_picker")),
        );

        manager.update(&mut model, Message::FileOpened(completion));

        assert_eq!(model.file_picker.visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        assert_eq!(model.context_stack.last(), Some(&Context::FilePicker));
    }

    #[test]
    fn picker_scans_previews_and_opens_the_selected_file() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        let file = nested.join("main.rs");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(&file, "fn main() {}").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(120, 20));
        let mut effect = None;
        for key in [' ', 'f', 'f'] {
            effect = manager.update(
                &mut model,
                Message::CommandMenuKey(crossterm::event::KeyCode::Char(key)),
            );
        }
        assert!(effect.is_none());
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        assert_eq!(model.context_stack.last(), Some(&Context::FilePicker));

        let Some(Effect::Feature(request)) = manager.update(&mut model, Message::TextInput('m'))
        else {
            panic!("a query should start one recursive scan");
        };
        let Some(Effect::Feature(request)) = manager.update(&mut model, request.perform()) else {
            panic!("recursive scan completion should start filtering");
        };
        let Some(Effect::Feature(request)) = manager.update(&mut model, request.perform()) else {
            panic!("filter completion should start a preview");
        };
        manager.update(&mut model, request.perform());
        assert_eq!(model.file_picker.results.len(), 1);
        let PreviewState::Text(snapshot) = &model.file_picker.preview else {
            panic!("text file should have a preview snapshot");
        };
        assert_eq!(snapshot.lines[0].text, "fn main() {}");
        assert!(snapshot.lines[0].highlights.iter().any(|span| {
            !span.highlights.is_empty() && span.start_byte == 0 && span.end_byte == 2
        }));
        assert!(snapshot.lines[0].highlights.iter().any(|span| {
            !span.highlights.is_empty() && span.start_byte == 3 && span.end_byte == 7
        }));

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_picker_file")),
        );
        complete_open(&mut model, &manager, effect);
        assert_eq!(model.file_picker.visibility, Visibility::Hidden);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(
            model.explorer.selected_entry().unwrap().path,
            std::fs::canonicalize(file).unwrap()
        );
        assert_eq!(
            model.explorer.entries[0].expansion,
            ExpansionState::Expanded
        );
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "fn main() {}"
        );
    }
}
