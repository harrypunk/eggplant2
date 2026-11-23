use crate::{
    data_object::{CommandId, Context, Model},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        input_context::input_context,
    },
};

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("provider_login_open"),
            "open provider login",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_close"),
            "close provider login",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "provider_login_open" => open(model),
        "provider_login_close" => close(model),
        _ => None,
    }
}

pub fn open(model: &mut Model) -> Option<Effect> {
    if model.provider_login.is_open() {
        return None;
    }
    let return_context = suspend_inline_agent(model);
    if !model.modal.open(Context::ProviderLogin) {
        resume_inline_agent(model, return_context);
        return None;
    }
    model.provider_login.open(return_context);
    model.context_stack.push(Context::ProviderLogin);
    None
}

fn suspend_inline_agent(model: &mut Model) -> Option<Context> {
    if input_context(model) != Some(Context::AgentChatDialog)
        || !model.agent.dialog_open()
        || !model.modal.is_active(Context::AgentChatDialog)
    {
        return None;
    }
    model.agent.suspend_dialog();
    model.modal.close(Context::AgentChatDialog);
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatDialog);
    Some(Context::AgentChatDialog)
}

fn close(model: &mut Model) -> Option<Effect> {
    let return_context = model.provider_login.close();
    model.modal.close(Context::ProviderLogin);
    model
        .context_stack
        .pop_if(|context| context == &Context::ProviderLogin);
    resume_inline_agent(model, return_context);
    None
}

fn resume_inline_agent(model: &mut Model, context: Option<Context>) {
    if context != Some(Context::AgentChatDialog)
        || !model.agent.dialog_open()
        || !model.modal.open(Context::AgentChatDialog)
    {
        return;
    }
    model.agent.resume_dialog();
    model.context_stack.push(Context::AgentChatDialog);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_owns_modal_input_until_closed() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        open(&mut model);
        assert!(model.provider_login.is_open());
        assert!(model.modal.is_active(Context::ProviderLogin));
        assert_eq!(model.context_stack.last(), Some(&Context::ProviderLogin));

        close(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(!model.modal.is_active(Context::ProviderLogin));
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn inline_agent_dialog_resumes_after_provider_login() {
        let mut model = Model::default();
        let buffer = crate::test_utils::create_buffer(&mut model, "fn main() {}\n");
        model.agent.open_dialog(
            crate::agent::EditorContext {
                buffer,
                path: std::path::PathBuf::from("/workspace/main.rs"),
                revision: 1,
                line: 1,
            },
            "main.rs:1".to_string(),
        );
        assert!(model.modal.open(Context::AgentChatDialog));
        model.context_stack.push(Context::AgentChatDialog);

        open(&mut model);
        assert!(model.provider_login.is_open());
        assert!(model.modal.is_active(Context::ProviderLogin));
        assert!(!model.modal.is_active(Context::AgentChatDialog));
        assert_eq!(model.context_stack.last(), Some(&Context::ProviderLogin));
        assert!(model.agent.dialog_open());
        assert_eq!(
            model.agent.dialog_state(),
            crate::agent::AgentDialogState::Suspended
        );

        close(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(model.agent.dialog_open());
        assert_eq!(
            model.agent.dialog_state(),
            crate::agent::AgentDialogState::Open
        );
        assert!(model.modal.is_active(Context::AgentChatDialog));
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChatDialog));
    }

    #[test]
    fn another_modal_prevents_opening() {
        let mut model = Model::default();
        assert!(model.modal.open(Context::FilePicker));
        open(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(model.modal.is_active(Context::FilePicker));
    }
}
