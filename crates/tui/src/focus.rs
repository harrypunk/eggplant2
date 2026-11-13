use crate::{
    data_object::{
        CommandId, Context, FocusDirection, FocusTarget, Model, StatusMessage, Visibility,
    },
    manager::effect::Effect,
};

/// Visibility and logical focus commands, independent of terminal rectangles.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "toggle_file_explorer" => toggle_file_explorer(model),
        "focus_left" => move_focus(model, FocusDirection::Left),
        "focus_down" => move_focus(model, FocusDirection::Down),
        "focus_up" => move_focus(model, FocusDirection::Up),
        "focus_right" => move_focus(model, FocusDirection::Right),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown focus command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

#[allow(clippy::single_match)]
fn toggle_file_explorer(model: &mut Model) {
    match model.explorer_visibility {
        Visibility::Hidden => match model.workspace {
            None => model.status = Some(StatusMessage::info("No folder open")),
            Some(_) => {
                model.explorer_visibility = Visibility::Displayed;
                focus_explorer(model);
            }
        },
        Visibility::Displayed => {
            model.explorer_visibility = Visibility::Hidden;
            match model.current_focus {
                FocusTarget::FileExplorer => {
                    model
                        .context_stack
                        .pop_if(|context| context == &Context::Explorer);
                    model.current_focus = FocusTarget::Editor;
                }
                _ => {}
            }
        }
    }
}

// The single-view layout only needs this explicit horizontal relationship.
// Future splits should navigate a logical layout tree, never Ratatui rectangles.
fn move_focus(model: &mut Model, direction: FocusDirection) {
    match (model.current_focus, direction, model.explorer_visibility) {
        (FocusTarget::Editor, FocusDirection::Left, Visibility::Displayed) => {
            focus_explorer(model);
        }
        (FocusTarget::FileExplorer, FocusDirection::Right, _) => {
            focus_editor(model);
        }
        _ => {}
    }
}

fn focus_explorer(model: &mut Model) {
    if model.workspace.is_none() {
        return;
    }

    model.current_focus = FocusTarget::FileExplorer;
    if model.context_stack.last() != Some(&Context::Explorer) {
        model.context_stack.push(Context::Explorer);
    }
}

pub(crate) fn focus_editor(model: &mut Model) {
    if model.editor.active_buffer().is_none() {
        return;
    }

    model.current_focus = FocusTarget::Editor;
    model
        .context_stack
        .pop_if(|context| context == &Context::Explorer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, startup::initial_model,
        test_utils::create_buffer,
    };

    #[test]
    fn hiding_and_showing_explorer_updates_focus() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer_visibility, Visibility::Hidden);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer_visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
    }

    #[test]
    fn directional_focus_moves_between_editor_and_explorer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        create_buffer(&mut model, "buffer");
        let manager = MessageManager::default();

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_right")));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_left")));
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));

        for command in ["focus_up", "focus_down"] {
            manager.update(&mut model, Message::InvokeCommand(CommandId(command)));
            assert_eq!(model.current_focus, FocusTarget::FileExplorer);
            assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        }
    }

    #[test]
    fn directional_focus_ignores_unavailable_targets() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_right")));
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));

        create_buffer(&mut model, "buffer");
        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_right")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_left")));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        let mut without_workspace = Model::default();
        without_workspace.context_stack.push(Context::AppNormal);
        create_buffer(&mut without_workspace, "buffer");
        manager.update(
            &mut without_workspace,
            Message::InvokeCommand(CommandId("focus_left")),
        );
        assert_eq!(without_workspace.current_focus, FocusTarget::Editor);
        assert_eq!(
            without_workspace.context_stack.last(),
            Some(&Context::AppNormal)
        );
    }
}
