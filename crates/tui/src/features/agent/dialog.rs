use super::{AgentContextProvider, EditorContext};
use crate::{
    data_object::{Context, Model},
    manager::effect::Effect,
};

/// Open the inline presentation without creating a separate Agent workflow.
pub(crate) fn open(model: &mut Model) -> Option<Effect> {
    if model.agent.dialog_open() {
        return None;
    }
    let context = match model.agent_context() {
        Ok(context) => context,
        Err(message) => return super::work::notify(model, message),
    };
    let input_context = match model.agent.session().pending_approval() {
        Some(_) => Context::AgentApproval,
        None => Context::AgentChatDialog,
    };
    if !model.modal.open(input_context) {
        return None;
    }
    let label = prompt_label(model, &context);
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

fn prompt_label(model: &Model, context: &EditorContext) -> String {
    let relative = model
        .workspace
        .as_ref()
        .and_then(|workspace| context.path.strip_prefix(workspace.root()).ok())
        .unwrap_or(&context.path);
    format!("{}:{}", relative.display(), context.line)
}
