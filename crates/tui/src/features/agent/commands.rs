use crate::{
    component::text_input::TextInputEvent,
    data_object::{CommandId, Context, GeneralAction, Model},
    manager::{
        command_registry::Command, effect::Effect, general_action::GeneralActionSubscription,
    },
};

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("agent_open"),
            "open AI agent",
            super::dialog::open,
        ),
        Command::feature(
            CommandId("agent_close"),
            "hide AI agent",
            super::dialog::close,
        ),
        Command::feature(
            CommandId("agent_submit"),
            "submit agent prompt",
            super::work::submit,
        ),
        Command::feature(
            CommandId("agent_interrupt"),
            "interrupt agent run",
            super::work::interrupt,
        ),
        Command::feature(
            CommandId("agent_scroll_up"),
            "scroll agent transcript up",
            super::navigation::page_up,
        ),
        Command::feature(
            CommandId("agent_scroll_down"),
            "scroll agent transcript down",
            super::navigation::page_down,
        ),
        Command::feature(
            CommandId("agent_cursor_up"),
            "move agent transcript cursor up",
            super::navigation::cursor_up,
        ),
        Command::feature(
            CommandId("agent_cursor_down"),
            "move agent transcript cursor down",
            super::navigation::cursor_down,
        ),
        Command::feature(
            CommandId("agent_toggle_panel"),
            "toggle agent tool panel",
            super::panel::toggle_panel,
        ),
        Command::feature(
            CommandId("agent_session_selector_open"),
            "select saved agent session",
            super::sessions::open,
        ),
        Command::feature(
            CommandId("agent_thinking_selector_open"),
            "select agent thinking options",
            super::thinking_level::open,
        ),
        Command::feature(
            CommandId("agent_panel_close"),
            "leave agent panel input",
            close_panel,
        ),
        Command::feature(
            CommandId("agent_insert"),
            "enter agent insert mode",
            super::panel::enter_insert_mode,
        ),
        Command::feature(
            CommandId("agent_panel_normal"),
            "return to agent normal mode",
            panel_normal,
        ),
        Command::feature(
            CommandId("agent_help"),
            "toggle agent help",
            super::panel::toggle_help,
        ),
        Command::feature(
            CommandId("agent_command_next"),
            "select next agent command",
            command_next,
        ),
        Command::feature(
            CommandId("agent_command_previous"),
            "select previous agent command",
            command_previous,
        ),
        Command::feature(
            CommandId("agent_command_quick_fill"),
            "quick fill agent command",
            command_quick_fill,
        ),
    ]
    .into_iter()
    .collect()
}

pub(crate) fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    let available = match super::surface::active(model) {
        Some(super::surface::AgentSurface::Dialog) => true,
        Some(super::surface::AgentSurface::Panel) => super::panel::insert_input_available(model),
        None => false,
    };
    if !available {
        return None;
    }
    let root = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf());
    let current = super::context::current_file(model);
    model.agent.handle_input_event(event);
    let Some(root) = root else {
        model.agent.dismiss_file_references();
        return None;
    };
    model
        .agent
        .update_file_references(&root, current)
        .map(Effect::feature)
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![
        GeneralActionSubscription::new(Context::AgentChatDialog, handle_general_action),
        GeneralActionSubscription::new(Context::AgentChatInsert, handle_general_action),
        GeneralActionSubscription::new(
            Context::AgentThinkingSelector,
            super::thinking_level::handle_general_action,
        ),
        GeneralActionSubscription::new(
            Context::AgentSessionSelector,
            super::sessions::handle_general_action,
        ),
    ]
}

pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => command_next(model),
        GeneralAction::Previous => command_previous(model),
        GeneralAction::Activate => activate(model),
        GeneralAction::Back => {
            if model.agent.file_references_active() {
                model.agent.dismiss_file_references();
                None
            } else {
                match super::surface::active(model) {
                    Some(super::surface::AgentSurface::Panel) => {
                        super::panel::enter_normal_mode(model)
                    }
                    Some(super::surface::AgentSurface::Dialog) => super::dialog::close(model),
                    None => None,
                }
            }
        }
        GeneralAction::FocusNext | GeneralAction::Paste => None,
    }
}

fn close_panel(model: &mut Model) -> Option<Effect> {
    super::panel::close_panel_input(model);
    None
}

fn panel_normal(model: &mut Model) -> Option<Effect> {
    if model.agent.help_visible() {
        super::panel::close_help(model)
    } else {
        super::panel::enter_normal_mode(model)
    }
}

pub(crate) fn command_next(model: &mut Model) -> Option<Effect> {
    if model.agent.file_references_active() {
        model.agent.select_next_file_reference();
        None
    } else {
        quick_fill(model, super::PromptCommandMovement::Next)
    }
}

pub(crate) fn command_previous(model: &mut Model) -> Option<Effect> {
    if model.agent.file_references_active() {
        model.agent.select_previous_file_reference();
        None
    } else {
        quick_fill(model, super::PromptCommandMovement::Previous)
    }
}

pub(crate) fn command_quick_fill(model: &mut Model) -> Option<Effect> {
    if model.agent.file_references_active() {
        model.agent.accept_file_reference();
        None
    } else {
        quick_fill(model, super::PromptCommandMovement::Current)
    }
}

fn activate(model: &mut Model) -> Option<Effect> {
    if model.agent.accept_file_reference() {
        None
    } else {
        super::work::submit(model)
    }
}

fn quick_fill(model: &mut Model, movement: super::PromptCommandMovement) -> Option<Effect> {
    model.agent.quick_fill_prompt_command(movement);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::command_registry::CommandRegistry;

    #[test]
    fn command_catalog_registers_direct_callbacks() {
        let registry = CommandRegistry::from_commands(commands()).unwrap();
        for id in [
            "agent_open",
            "agent_close",
            "agent_submit",
            "agent_interrupt",
            "agent_scroll_up",
            "agent_scroll_down",
            "agent_cursor_up",
            "agent_cursor_down",
            "agent_toggle_panel",
            "agent_session_selector_open",
            "agent_thinking_selector_open",
            "agent_panel_close",
            "agent_insert",
            "agent_panel_normal",
            "agent_help",
            "agent_command_next",
            "agent_command_previous",
            "agent_command_quick_fill",
        ] {
            assert!(registry.get(id).is_some(), "missing {id}");
        }
    }
}
