use std::{fmt::Display, path::Path};

use crate::{
    data_object::{CommandId, Model, StatusMessage},
    manager::effect::Effect,
};

use super::{ExpansionState, IgnoreMode};

/// Explorer state commands. File opening and panel focus stay in separate slices.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "explorer_next" => model.explorer.select_next(),
        "explorer_previous" => model.explorer.select_previous(),
        "explorer_fold" => change_expansion(model, ExpansionState::Collapsed),
        "explorer_unfold" => change_expansion(model, ExpansionState::Expanded),
        "refresh_explorer" => refresh(model),
        "toggle_ignored_files" => toggle_ignored_files(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown explorer command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

fn change_expansion(model: &mut Model, target: ExpansionState) {
    let Some(workspace) = &model.workspace else {
        return;
    };
    let Some(entry) = model.explorer.selected_entry() else {
        return;
    };
    let path = entry.path.clone();
    let result = match target {
        ExpansionState::Expanded => model.explorer.unfold_selected(workspace.root()),
        ExpansionState::Collapsed => model.explorer.fold_selected(workspace.root()),
    };
    match result {
        Ok(true) => model.status = None,
        Ok(false) => {}
        Err(error) => report_error(model, "failed to update directory tree", &path, error),
    }
}

fn refresh(model: &mut Model) {
    let Some(workspace) = &model.workspace else {
        return;
    };
    let root = workspace.root().to_path_buf();

    match model.explorer.reload(&root) {
        Ok(()) => model.status = None,
        Err(error) => report_error(model, "failed to refresh", &root, error),
    }
}

fn toggle_ignored_files(model: &mut Model) {
    let Some(workspace) = &model.workspace else {
        return;
    };
    let root = workspace.root().to_path_buf();

    match model.explorer.toggle_ignore_mode(&root) {
        Ok(mode) => {
            let text = match mode {
                IgnoreMode::HideIgnored => "Hiding ignored files",
                IgnoreMode::ShowIgnored => "Showing ignored files",
            };
            model.status = Some(StatusMessage::info(text));
        }
        Err(error) => report_error(model, "failed to toggle ignored files", &root, error),
    }
}

fn report_error(model: &mut Model, action: &str, path: &Path, error: impl Display) {
    model.status = Some(StatusMessage::error(format!(
        "{action} `{}`: {error}",
        path.display()
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{Message, StatusKind},
        manager::message_manager::MessageManager,
        startup::initial_model,
    };

    #[test]
    fn unfold_and_fold_preserve_the_workspace_root() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(child.join("note.txt"), "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        let workspace_root = model.workspace.as_ref().unwrap().root().to_path_buf();

        manager.update(&mut model, Message::OpenSelected);
        assert_eq!(model.explorer.entries.len(), 2);
        assert_eq!(model.explorer.entries[1].depth, 1);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_fold")),
        );
        assert_eq!(model.explorer.entries.len(), 1);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_unfold")),
        );
        assert_eq!(model.explorer.entries.len(), 2);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);
    }

    #[test]
    fn refresh_reloads_the_workspace_tree() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        assert!(model.explorer.entries.is_empty());

        std::fs::write(directory.path().join("new.txt"), "new").unwrap();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("refresh_explorer")),
        );

        assert_eq!(model.explorer.entries.len(), 1);
        assert_eq!(model.explorer.selected, Some(0));
    }

    #[test]
    fn ignored_entries_follow_toggle_commands() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(directory.path().join("ignored.txt"), "ignored").unwrap();
        std::fs::write(directory.path().join("visible.txt"), "visible").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_ignored_files")),
        );
        assert!(
            model
                .explorer
                .entries
                .iter()
                .any(|entry| entry.name == "ignored.txt")
        );
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Showing ignored files")
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_ignored_files")),
        );
        assert!(
            model
                .explorer
                .entries
                .iter()
                .all(|entry| entry.name != "ignored.txt")
        );
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Hiding ignored files")
        );
    }

    #[test]
    fn expansion_failure_preserves_the_visible_tree() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        let original_entries = model.explorer.entries.clone();
        std::fs::remove_dir(&child).unwrap();
        std::fs::remove_dir(directory.path()).unwrap();

        manager.update(&mut model, Message::OpenSelected);

        let status = model.status.as_ref().unwrap();
        assert_eq!(status.kind, StatusKind::Error);
        assert!(status.text.contains("failed to update directory tree"));
        assert_eq!(model.explorer.entries, original_entries);
    }
}
