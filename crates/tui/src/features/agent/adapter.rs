use crate::{
    data_object::{Model, StatusMessage},
    manager::effect::Effect,
};

use super::{
    AgentState,
    tools::ToolContext,
    work::{InterruptionContext, SubmissionContext},
};

impl SubmissionContext for Model {
    fn agent(&self) -> &AgentState {
        &self.agent
    }

    fn agent_mut(&mut self) -> &mut AgentState {
        &mut self.agent
    }

    fn submission_active(&self) -> bool {
        super::surface::active(self).is_some()
    }

    fn project_available(&self) -> bool {
        self.workspace.is_some()
    }

    fn tool_context(&self) -> Option<ToolContext> {
        super::context::tool_context(self)
    }

    fn open_provider_login(&mut self) -> Option<Effect> {
        crate::provider_login::open(self)
    }

    fn open_provider_logout(&mut self) -> Option<Effect> {
        crate::provider_logout::open(self)
    }

    fn open_model_selector(&mut self) -> Option<Effect> {
        crate::model_selector::open(self)
    }

    fn notify(&mut self, message: String) -> Option<Effect> {
        notify(self, message)
    }
}

impl InterruptionContext for Model {
    fn agent(&self) -> &AgentState {
        &self.agent
    }

    fn agent_mut(&mut self) -> &mut AgentState {
        &mut self.agent
    }

    fn transition_to_chat(&mut self) {
        super::approval::transition_to_chat(self);
    }

    fn persist_finalized(&mut self) -> Option<Effect> {
        super::storage::persist_finalized(self)
    }
}

pub(super) fn notify(model: &mut Model, message: String) -> Option<Effect> {
    Some(crate::notifications::publish_status(
        model,
        StatusMessage::info(message),
        Some("Agent"),
    ))
}
