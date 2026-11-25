use super::EditorContext;
use crate::{
    data_object::{Context, Model},
    manager::effect::Effect,
};

/// Open the inline presentation without creating a separate Agent workflow.
pub(crate) fn open(model: &mut Model) -> Option<Effect> {
    if model.agent.dialog_open() {
        return None;
    }
    if model.workspace.is_none() {
        return super::work::notify(model, "Open a project to use the agent".to_string());
    }
    let context = super::context::editor_context(model);
    let input_context = match model.agent.session().pending_approval() {
        Some(_) => Context::AgentApproval,
        None => Context::AgentChatDialog,
    };
    if !model.modal.open(input_context) {
        return None;
    }
    let label = context
        .as_ref()
        .map(|context| prompt_label(model, context))
        .unwrap_or_default();
    model.agent.open_dialog(context, label);
    model.context_stack.push(input_context);
    None
}

pub(crate) fn close(model: &mut Model) -> Option<Effect> {
    if !model.agent.dialog_open() {
        return None;
    }
    model.agent.close_dialog();
    [Context::AgentChatDialog, Context::AgentApproval]
        .into_iter()
        .for_each(|context| {
            model.modal.close(context);
            model.context_stack.pop_if(|active| active == &context);
        });
    None
}

pub(crate) fn suspend_for_overlay(model: &mut Model) -> Option<Context> {
    use crate::manager::input_context::input_context;

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

pub(crate) fn resume_overlay(model: &mut Model, context: Option<Context>) {
    if context != Some(Context::AgentChatDialog)
        || !model.agent.dialog_open()
        || !model.modal.open(Context::AgentChatDialog)
    {
        return;
    }
    model.agent.resume_dialog();
    model.context_stack.push(Context::AgentChatDialog);
}

fn prompt_label(model: &Model, context: &EditorContext) -> String {
    let relative = model
        .workspace
        .as_ref()
        .and_then(|workspace| context.path.strip_prefix(workspace.root()).ok())
        .unwrap_or(&context.path);
    format!("{}:{}", relative.display(), context.line)
}
