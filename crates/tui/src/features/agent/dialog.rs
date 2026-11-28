use super::{AgentUiAction, DialogAction, EditorContext};
use crate::{
    data_object::{Context, Model},
    manager::effect::Effect,
};

enum DialogOpenPlan {
    AlreadyOpen,
    ProjectRequired,
    Ready { context: Option<EditorContext> },
}

impl DialogOpenPlan {
    fn evaluate(model: &Model) -> Self {
        match (model.agent.dialog_open(), model.workspace.is_some()) {
            (true, _) => Self::AlreadyOpen,
            (_, false) => Self::ProjectRequired,
            (false, true) => Self::Ready {
                context: super::context::editor_context(model),
            },
        }
    }
}

/// Open the inline presentation without creating a separate Agent workflow.
pub(crate) fn open(model: &mut Model) -> Option<Effect> {
    match DialogOpenPlan::evaluate(model) {
        DialogOpenPlan::AlreadyOpen => None,
        DialogOpenPlan::ProjectRequired => {
            super::adapter::notify(model, "Open a project to use the agent".to_string())
        }
        DialogOpenPlan::Ready { context } => match model.modal.open(Context::AgentChatDialog) {
            false => None,
            true => {
                model.agent.dispatch(AgentUiAction::OpenDialog { context });
                None
            }
        },
    }
}

pub(crate) fn close(model: &mut Model) -> Option<Effect> {
    match model.agent.dialog_open() {
        false => None,
        true => {
            model
                .agent
                .dispatch(AgentUiAction::Dialog(DialogAction::Close));
            model.modal.close(Context::AgentChatDialog);
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
    match (
        model.agent.dialog_open(),
        model.modal.is_active(Context::AgentChatDialog),
    ) {
        (true, true) => OverlayTransition::Suspend,
        _ => OverlayTransition::Skip,
    }
}

pub(crate) fn suspend_for_overlay(model: &mut Model) -> Option<Context> {
    match suspension(model) {
        OverlayTransition::Skip | OverlayTransition::Resume => None,
        OverlayTransition::Suspend => {
            model
                .agent
                .dispatch(AgentUiAction::Dialog(DialogAction::Suspend));
            model.modal.close(Context::AgentChatDialog);
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
        OverlayTransition::Resume => model
            .agent
            .dispatch(AgentUiAction::Dialog(DialogAction::Resume)),
        OverlayTransition::Skip | OverlayTransition::Suspend => {}
    }
}
