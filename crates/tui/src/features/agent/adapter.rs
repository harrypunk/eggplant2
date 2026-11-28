use std::path::Path;

use editor::BufferId;
use eggplant_agent::ToolCall;

use crate::{
    data_object::{Model, StatusMessage},
    manager::effect::Effect,
};

use super::{
    mutation::MutationContext,
    runtime::AgentRuntime,
    tools::ToolContext,
    ui_state::AgentUiState,
    work::{InterruptionContext, SubmissionContext},
    write::{MutationKind, MutationOutcome},
};

impl SubmissionContext for Model {
    fn runtime(&self) -> &AgentRuntime {
        self.agent.runtime()
    }

    fn runtime_mut(&mut self) -> &mut AgentRuntime {
        self.agent.runtime_mut()
    }

    fn ui(&self) -> &AgentUiState {
        self.agent.ui()
    }

    fn ui_mut(&mut self) -> &mut AgentUiState {
        self.agent.ui_mut()
    }

    fn submission_surface(&self) -> Option<super::surface::AgentSurface> {
        super::surface::active(self)
    }

    fn project_available(&self) -> bool {
        self.workspace.is_some()
    }

    fn tool_context(&self) -> Option<ToolContext> {
        super::context::tool_context(self)
    }

    fn execute_prompt_command(
        &mut self,
        invocation: super::PromptCommandInvocation,
    ) -> Option<Effect> {
        super::execute(self, &invocation)
    }

    fn notify(&mut self, message: String) -> Option<Effect> {
        notify(self, message)
    }
}

impl InterruptionContext for Model {
    fn runtime(&self) -> &AgentRuntime {
        self.agent.runtime()
    }

    fn runtime_mut(&mut self) -> &mut AgentRuntime {
        self.agent.runtime_mut()
    }

    fn persist_finalized(&mut self) -> Option<Effect> {
        super::storage::persist_finalized(self)
    }
}

impl MutationContext for Model {
    fn runtime(&self) -> &AgentRuntime {
        self.agent.runtime()
    }

    fn runtime_mut(&mut self) -> &mut AgentRuntime {
        self.agent.runtime_mut()
    }

    fn tool_context(&self) -> Option<ToolContext> {
        super::context::tool_context(self)
    }

    fn apply_mutation(
        &mut self,
        kind: MutationKind,
        root: &Path,
        call: &ToolCall,
    ) -> MutationOutcome {
        kind.apply(&mut self.editor, root, call)
    }

    fn after_mutation(&mut self, edited: Option<BufferId>) -> Option<Effect> {
        edited.and_then(|buffer_id| {
            Effect::combine(
                crate::buffer::save_if_dirty(self, buffer_id),
                crate::code_navigation::synchronize_buffer(self, buffer_id),
            )
        })
    }
}

pub(super) fn notify(model: &mut Model, message: String) -> Option<Effect> {
    Some(crate::notifications::publish_status(
        model,
        StatusMessage::info(message),
        Some("Agent"),
    ))
}
