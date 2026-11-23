use std::path::Path;

use crate::{
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::effect::Effect,
};

use super::{
    ExpansionState, TreeChange, add, delete,
    transfer::{self, TransferOperation},
    work::ExplorerRefreshKind,
};

/// Explorer state commands. File opening and panel focus stay in separate slices.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "explorer_next" => model.explorer.select_next(),
        "explorer_previous" => model.explorer.select_previous(),
        "explorer_fold" => return change_expansion(model, ExpansionState::Collapsed),
        "explorer_unfold" => return change_expansion(model, ExpansionState::Expanded),
        "refresh_explorer" => return refresh(model),
        "toggle_ignored_files" => return toggle_ignored_files(model),
        "explorer_help" => return toggle_help(model),
        "close_explorer_help" => return close_help(model),
        "explorer_add" => return add::open(model),
        "explorer_add_submit" => return add::submit(model),
        "close_explorer_add" => return add::cancel(model),
        "explorer_delete" => return delete::open(model),
        "explorer_delete_confirm" => return delete::confirm(model),
        "close_explorer_delete" => return delete::cancel(model),
        "explorer_move" => return transfer::open(model, TransferOperation::Move),
        "explorer_transfer_submit" => return transfer::submit(model),
        "close_explorer_transfer" => return transfer::cancel(model),
        "explorer_copy" => return transfer::open(model, TransferOperation::Copy),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown explorer command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

fn change_expansion(model: &mut Model, target: ExpansionState) -> Option<Effect> {
    if target == ExpansionState::Collapsed {
        if model.explorer.fold_selected() == TreeChange::Changed {
            model.status = None;
        }
        return None;
    }

    let root = model.workspace.as_ref()?.root().to_path_buf();
    model.explorer.unfold_request(&root).map(Effect::feature)
}

fn refresh(model: &mut Model) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    Some(Effect::feature(
        model
            .explorer
            .reload_request(&root, ExplorerRefreshKind::Manual),
    ))
}

/// Refresh the tree after an external create/delete/rename without clearing unrelated status.
pub(crate) fn refresh_changed(model: &mut Model, path: &Path) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    if path.strip_prefix(&root).is_err() {
        return None;
    }
    Some(Effect::feature(
        model
            .explorer
            .reload_request(&root, ExplorerRefreshKind::External),
    ))
}

fn toggle_ignored_files(model: &mut Model) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    Some(Effect::feature(model.explorer.toggle_ignore_request(&root)))
}

fn toggle_help(model: &mut Model) -> Option<Effect> {
    if model.explorer.help_visible() {
        return close_help(model);
    }
    if !model.modal.open(Context::ExplorerHelp) {
        return None;
    }
    model.explorer.open_help();
    model.context_stack.push(Context::ExplorerHelp);
    None
}

fn close_help(model: &mut Model) -> Option<Effect> {
    model.explorer.close_help();
    model.modal.close(Context::ExplorerHelp);
    model
        .context_stack
        .pop_if(|context| context == &Context::ExplorerHelp);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{Message, StatusKind},
        manager::message_manager::MessageManager,
        startup::initial_model,
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

    #[test]
    fn unfold_and_fold_preserve_the_workspace_root() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(child.join("note.txt"), "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        let workspace_root = model.workspace.as_ref().unwrap().root().to_path_buf();

        let effect = manager.update(&mut model, Message::OpenSelected);
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }
        assert_eq!(model.explorer.entries.len(), 2);
        assert_eq!(model.explorer.entries[1].depth, 1);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_fold")),
        );
        assert!(effect.is_none());
        assert_eq!(model.explorer.entries.len(), 1);
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_unfold")),
        );
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }
        assert_eq!(model.explorer.entries.len(), 2);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);

        model.explorer.selected = Some(1);
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_fold")),
        );
        assert!(effect.is_none());
        assert_eq!(model.explorer.entries.len(), 1);
        assert_eq!(model.explorer.selected, Some(0));
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);
    }

    #[test]
    fn refresh_reloads_the_workspace_tree() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        assert!(model.explorer.entries.is_empty());

        std::fs::write(directory.path().join("new.txt"), "new").unwrap();
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("refresh_explorer")),
        );
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }

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

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_ignored_files")),
        );
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }
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

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_ignored_files")),
        );
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }
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
    fn help_owns_a_modal_context_until_toggled_or_closed() {
        let mut model = Model::default();
        model.context_stack.push(Context::Explorer);
        let manager = MessageManager::default();

        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(CommandId("explorer_help"))
                )
                .is_none()
        );
        assert!(model.explorer.help_visible());
        assert_eq!(model.context_stack.last(), Some(&Context::ExplorerHelp));
        assert!(model.modal.is_active(Context::ExplorerHelp));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_help")),
        );
        assert!(!model.explorer.help_visible());
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        assert!(!model.modal.is_active(Context::ExplorerHelp));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_help")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_explorer_help")),
        );
        assert!(!model.explorer.help_visible());
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
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

        let effect = manager.update(&mut model, Message::OpenSelected);
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }

        let status = model.status.as_ref().unwrap();
        assert_eq!(status.kind, StatusKind::Error);
        assert!(status.text.contains("failed to update directory tree"));
        assert_eq!(model.explorer.entries, original_entries);
    }
}
