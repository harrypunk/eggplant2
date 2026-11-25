use super::EditorContext;
use crate::{
    data_object::{Context, Model},
    manager::{effect::Effect, input_context::input_context},
};

enum DialogOpenPlan {
    AlreadyOpen,
    ProjectRequired,
    Ready {
        context: Option<EditorContext>,
        input_context: Context,
        label: String,
    },
}

impl DialogOpenPlan {
    fn evaluate(model: &Model) -> Self {
        match (model.agent.dialog_open(), model.workspace.is_some()) {
            (true, _) => Self::AlreadyOpen,
            (_, false) => Self::ProjectRequired,
            (false, true) => {
                let context = super::context::editor_context(model);
                let input_context = match model.agent.session().pending_approval() {
                    Some(_) => Context::AgentApproval,
                    None => Context::AgentChatDialog,
                };
                let label = context
                    .as_ref()
                    .map(|context| prompt_label(model, context))
                    .unwrap_or_default();
                Self::Ready {
                    context,
                    input_context,
                    label,
                }
            }
        }
    }
}

/// Open the inline presentation without creating a separate Agent workflow.
pub(crate) fn open(model: &mut Model) -> Option<Effect> {
    match DialogOpenPlan::evaluate(model) {
        DialogOpenPlan::AlreadyOpen => None,
        DialogOpenPlan::ProjectRequired => {
            super::work::notify(model, "Open a project to use the agent".to_string())
        }
        DialogOpenPlan::Ready {
            context,
            input_context,
            label,
        } => match model.modal.open(input_context) {
            false => None,
            true => {
                model.agent.open_dialog(context, label);
                model.context_stack.push(input_context);
                None
            }
        },
    }
}

pub(crate) fn close(model: &mut Model) -> Option<Effect> {
    match model.agent.dialog_open() {
        false => None,
        true => {
            model.agent.close_dialog();
            [Context::AgentChatDialog, Context::AgentApproval]
                .into_iter()
                .for_each(|context| {
                    model.modal.close(context);
                    model.context_stack.pop_if(|active| active == &context);
                });
            None
        }
    }
}

enum OverlayTransition {
    Skip,
    Suspend,
    Resume,
}

fn suspension(model: &Model) -> OverlayTransition {
    match input_context(model) {
        Some(Context::AgentChatDialog)
            if model.agent.dialog_open() && model.modal.is_active(Context::AgentChatDialog) =>
        {
            OverlayTransition::Suspend
        }
        _ => OverlayTransition::Skip,
    }
}

pub(crate) fn suspend_for_overlay(model: &mut Model) -> Option<Context> {
    match suspension(model) {
        OverlayTransition::Skip | OverlayTransition::Resume => None,
        OverlayTransition::Suspend => {
            model.agent.suspend_dialog();
            model.modal.close(Context::AgentChatDialog);
            model
                .context_stack
                .pop_if(|context| context == &Context::AgentChatDialog);
            Some(Context::AgentChatDialog)
        }
    }
}

pub(crate) fn resume_overlay(model: &mut Model, context: Option<Context>) {
    let transition = match (context, model.agent.dialog_open()) {
        (Some(Context::AgentChatDialog), true) => {
            match model.modal.open(Context::AgentChatDialog) {
                true => OverlayTransition::Resume,
                false => OverlayTransition::Skip,
            }
        }
        _ => OverlayTransition::Skip,
    };
    match transition {
        OverlayTransition::Resume => {
            model.agent.resume_dialog();
            model.context_stack.push(Context::AgentChatDialog);
        }
        OverlayTransition::Skip | OverlayTransition::Suspend => {}
    }
}

fn prompt_label(model: &Model, context: &EditorContext) -> String {
    let relative = model
        .workspace
        .as_ref()
        .and_then(|workspace| context.path.strip_prefix(workspace.root()).ok())
        .unwrap_or(&context.path);
    format!("{}:{}", relative.display(), context.line)
}
