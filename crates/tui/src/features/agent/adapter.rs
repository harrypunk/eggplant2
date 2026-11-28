use editor::{DocumentFactory, EditorChange, ExactTextReplacement};

use crate::{
    data_object::{Model, StatusMessage},
    manager::effect::Effect,
};

use super::{
    execution::ToolExecutionContext,
    host::{AgentEditorError, AgentHost, CreateTextFile},
    runtime::AgentRuntime,
    tools::ToolContext,
    ui_state::AgentUiState,
    work::{InterruptionContext, SubmissionContext},
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

impl ToolExecutionContext for Model {
    fn runtime(&self) -> &AgentRuntime {
        self.agent.runtime()
    }

    fn runtime_mut(&mut self) -> &mut AgentRuntime {
        self.agent.runtime_mut()
    }

    fn tool_context(&self) -> Option<ToolContext> {
        super::context::tool_context(self)
    }

    fn host(&mut self) -> &mut dyn AgentHost {
        self
    }

    fn after_edit(&mut self, change: Option<EditorChange>) -> Option<Effect> {
        change.and_then(|change| {
            Effect::combine(
                crate::buffer::save_if_dirty(self, change.buffer_id),
                crate::code_navigation::synchronize_buffer(self, change.buffer_id),
            )
        })
    }
}

impl AgentHost for Model {
    fn replace_exact_text(
        &mut self,
        request: ExactTextReplacement<'_>,
    ) -> Result<EditorChange, AgentEditorError> {
        self.editor
            .replace_exact_text(request)
            .map_err(|error| AgentEditorError::new(error.to_string()))
    }

    fn create_text_file(
        &mut self,
        request: CreateTextFile,
    ) -> Result<EditorChange, AgentEditorError> {
        let relative = request.target().relative().to_path_buf();
        let path =
            crate::filesystem::create(request.root(), request.target()).map_err(|error| {
                AgentEditorError::new(format!(
                    "failed to create `{}`: {error}",
                    relative.display()
                ))
            })?;
        let document = DocumentFactory::new()
            .open(&path)
            .map_err(|error| AgentEditorError::new(error.to_string()))?;
        self.editor
            .create_file_document(document, request.content())
            .map_err(|error| AgentEditorError::new(error.to_string()))
    }
}

pub(super) fn notify(model: &mut Model, message: String) -> Option<Effect> {
    Some(crate::notifications::publish_status(
        model,
        StatusMessage::info(message),
        Some("Agent"),
    ))
}
