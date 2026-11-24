use crate::{
    component::text_input::TextInputEvent,
    data_object::{CommandId, Context, GeneralAction, Model},
    manager::{
        command_registry::Command, effect::Effect, general_action::GeneralActionSubscription,
        input_context::input_context,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentCommand {
    Open,
    Close,
    Submit,
    Interrupt,
    Approve,
    Reject,
    TogglePanel,
    PanelClose,
    Insert,
    PanelNormal,
    Help,
    CommandNext,
    CommandPrevious,
    CommandQuickFill,
}

impl AgentCommand {
    const ALL: [Self; 14] = [
        Self::Open,
        Self::Close,
        Self::Submit,
        Self::Interrupt,
        Self::Approve,
        Self::Reject,
        Self::TogglePanel,
        Self::PanelClose,
        Self::Insert,
        Self::PanelNormal,
        Self::Help,
        Self::CommandNext,
        Self::CommandPrevious,
        Self::CommandQuickFill,
    ];

    const fn id(self) -> CommandId {
        match self {
            Self::Open => CommandId("agent_open"),
            Self::Close => CommandId("agent_close"),
            Self::Submit => CommandId("agent_submit"),
            Self::Interrupt => CommandId("agent_interrupt"),
            Self::Approve => CommandId("agent_approve"),
            Self::Reject => CommandId("agent_reject"),
            Self::TogglePanel => CommandId("agent_toggle_panel"),
            Self::PanelClose => CommandId("agent_panel_close"),
            Self::Insert => CommandId("agent_insert"),
            Self::PanelNormal => CommandId("agent_panel_normal"),
            Self::Help => CommandId("agent_help"),
            Self::CommandNext => CommandId("agent_command_next"),
            Self::CommandPrevious => CommandId("agent_command_previous"),
            Self::CommandQuickFill => CommandId("agent_command_quick_fill"),
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Open => "open AI agent",
            Self::Close => "hide AI agent",
            Self::Submit => "submit agent prompt",
            Self::Interrupt => "interrupt agent run",
            Self::Approve => "approve agent write",
            Self::Reject => "reject agent write",
            Self::TogglePanel => "toggle agent tool panel",
            Self::PanelClose => "leave agent panel input",
            Self::Insert => "enter agent insert mode",
            Self::PanelNormal => "return to agent normal mode",
            Self::Help => "toggle agent help",
            Self::CommandNext => "select next agent command",
            Self::CommandPrevious => "select previous agent command",
            Self::CommandQuickFill => "quick fill agent command",
        }
    }

    fn from_id(id: CommandId) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }
}

pub fn commands() -> Vec<Command> {
    AgentCommand::ALL
        .into_iter()
        .map(|command| Command::feature(command.id(), command.description(), handle_command))
        .collect()
}

pub(crate) fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    let context = input_context(model)?;
    if ![Context::AgentChatDialog, Context::AgentChatInsert].contains(&context) {
        return None;
    }
    model.agent.handle_input_event(event);
    None
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![
        GeneralActionSubscription::new(Context::AgentChatDialog, handle_general_action),
        GeneralActionSubscription::new(Context::AgentChatInsert, handle_general_action),
    ]
}

pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => {
            model
                .agent
                .quick_fill_prompt_command(super::PromptCommandMovement::Next);
            None
        }
        GeneralAction::Previous => {
            model
                .agent
                .quick_fill_prompt_command(super::PromptCommandMovement::Previous);
            None
        }
        GeneralAction::Activate => super::work::submit(model),
        GeneralAction::Back => match input_context(model) {
            Some(Context::AgentChatInsert) => super::panel::enter_normal_mode(model),
            Some(Context::AgentChatDialog) => super::dialog::close(model),
            _ => None,
        },
    }
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match AgentCommand::from_id(id)? {
        AgentCommand::Open => super::dialog::open(model),
        AgentCommand::Close => super::dialog::close(model),
        AgentCommand::Submit => super::work::submit(model),
        AgentCommand::Interrupt => super::work::interrupt(model),
        AgentCommand::Approve => super::approval::approve_write(model),
        AgentCommand::Reject => super::approval::reject_write(model),
        AgentCommand::TogglePanel => super::panel::toggle_panel(model),
        AgentCommand::PanelClose => {
            super::panel::close_panel_input(model);
            None
        }
        AgentCommand::Insert => super::panel::enter_insert_mode(model),
        AgentCommand::PanelNormal => {
            if model.agent.help_visible() {
                super::panel::close_help(model)
            } else {
                super::panel::enter_normal_mode(model)
            }
        }
        AgentCommand::Help => super::panel::toggle_help(model),
        AgentCommand::CommandNext => {
            model
                .agent
                .quick_fill_prompt_command(super::PromptCommandMovement::Next);
            None
        }
        AgentCommand::CommandPrevious => {
            model
                .agent
                .quick_fill_prompt_command(super::PromptCommandMovement::Previous);
            None
        }
        AgentCommand::CommandQuickFill => {
            model
                .agent
                .quick_fill_prompt_command(super::PromptCommandMovement::Current);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_round_trip() {
        for command in AgentCommand::ALL {
            assert_eq!(AgentCommand::from_id(command.id()), Some(command));
        }
        assert_eq!(AgentCommand::from_id(CommandId("unknown")), None);
    }
}
