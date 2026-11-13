use crate::{
    data_object::{CommandId, Context, FocusTarget, Model, StatusMessage},
    document,
    manager::effect::Effect,
};

use super::{FileEntry, PreviewState};

/// File-picker commands and completion coordination.
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
            document::open_file(&documents, model, &path)
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

pub(crate) fn complete_scan(
    model: &mut Model,
    revision: u64,
    result: Result<Vec<FileEntry>, String>,
) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    model
        .file_picker
        .complete_scan(&root, revision, result)
        .map(Effect::feature)
}

pub(crate) fn complete_filter(
    model: &mut Model,
    revision: u64,
    results: Vec<FileEntry>,
) -> Option<Effect> {
    model
        .file_picker
        .complete_filter(revision, results)
        .map(Effect::feature)
}

pub(crate) fn complete_preview(model: &mut Model, revision: u64, preview: PreviewState) {
    model.file_picker.complete_preview(revision, preview);
}

pub(crate) fn close(model: &mut Model) {
    model.file_picker.close();
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
        manager::message_manager::MessageManager,
        startup::initial_model,
    };

    fn complete_open(model: &mut Model, manager: &MessageManager, effect: Option<Effect>) {
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("expected an open-document effect");
        };
        manager.update(model, Message::FileOpened(request.perform()));
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
