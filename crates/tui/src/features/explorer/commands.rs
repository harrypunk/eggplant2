use std::path::Path;

use editor::DocumentFactory;

use crate::{
    data_object::{Context, Model},
    document,
    manager::effect::Effect,
};

use super::{
    ExpansionState, ExplorerEntryKind, TreeChange, add, delete,
    transfer::{self, TransferOperation},
    work::ExplorerRefreshKind,
};

/// Open the selected entry through the document capability, or expand a directory.
pub(crate) fn open_selected(documents: &DocumentFactory, model: &mut Model) -> Option<Effect> {
    let entry = model.explorer.selected_entry()?;
    let path = entry.path.clone();
    match entry.kind {
        ExplorerEntryKind::File | ExplorerEntryKind::Link | ExplorerEntryKind::Other => {
            document::open_file(documents, model, &path)
        }
        ExplorerEntryKind::Directory => unfold(model),
    }
}

pub fn next(model: &mut Model) -> Option<Effect> {
    model.explorer.select_next();
    None
}

pub fn previous(model: &mut Model) -> Option<Effect> {
    model.explorer.select_previous();
    None
}

pub fn fold(model: &mut Model) -> Option<Effect> {
    change_expansion(model, ExpansionState::Collapsed)
}

pub fn unfold(model: &mut Model) -> Option<Effect> {
    change_expansion(model, ExpansionState::Expanded)
}

pub fn open_add(model: &mut Model) -> Option<Effect> {
    add::open(model)
}

pub fn submit_add(model: &mut Model) -> Option<Effect> {
    add::submit(model)
}

pub fn cancel_add(model: &mut Model) -> Option<Effect> {
    add::cancel(model)
}

pub fn open_delete(model: &mut Model) -> Option<Effect> {
    delete::open(model)
}

pub fn confirm_delete(model: &mut Model) -> Option<Effect> {
    delete::confirm(model)
}

pub fn cancel_delete(model: &mut Model) -> Option<Effect> {
    delete::cancel(model)
}

pub fn open_move(model: &mut Model) -> Option<Effect> {
    transfer::open(model, TransferOperation::Move)
}

pub fn open_copy(model: &mut Model) -> Option<Effect> {
    transfer::open(model, TransferOperation::Copy)
}

pub fn submit_transfer(model: &mut Model) -> Option<Effect> {
    transfer::submit(model)
}

pub fn cancel_transfer(model: &mut Model) -> Option<Effect> {
    transfer::cancel(model)
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

pub fn refresh(model: &mut Model) -> Option<Effect> {
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

pub fn toggle_ignored_files(model: &mut Model) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    Some(Effect::feature(model.explorer.toggle_ignore_request(&root)))
}

pub fn toggle_help(model: &mut Model) -> Option<Effect> {
    if model.explorer.help_visible() {
        return close_help(model);
    }
    if !model.modal.open(Context::ExplorerHelp) {
        return None;
    }
    model.explorer.open_help();
    None
}

pub fn close_help(model: &mut Model) -> Option<Effect> {
    model.explorer.close_help();
    model.modal.close(Context::ExplorerHelp);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{CommandId, FocusTarget, Message, StatusKind},
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

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        );
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
        let mut model = Model {
            current_focus: FocusTarget::FileExplorer,
            ..Model::default()
        };
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
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::ExplorerHelp)
        );
        assert!(model.context_stack.is_empty());
        assert!(model.modal.is_active(Context::ExplorerHelp));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_help")),
        );
        assert!(!model.explorer.help_visible());
        assert!(model.context_stack.is_empty());
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
        assert!(model.context_stack.is_empty());
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

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        );
        if let Some(effect) = effect {
            apply(&mut model, effect);
        }

        let status = model.status.as_ref().unwrap();
        assert_eq!(status.kind, StatusKind::Error);
        assert!(status.text.contains("failed to update directory tree"));
        assert_eq!(model.explorer.entries, original_entries);
    }
}
