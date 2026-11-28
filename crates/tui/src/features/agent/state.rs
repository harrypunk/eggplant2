use std::sync::Arc;

use eggplant_agent::{AgentContinuation, AgentProvider, AgentSession, CancellationToken, RunId};
use eggplant_agent_storage::SessionStore;

use super::{
    AgentDialogState, AgentInputMode, EditorContext, FileReferenceView, PromptCommandMovement,
    PromptCommandState,
    runtime::AgentRuntime,
    sessions::SessionSelectorState,
    surface::AgentSurface,
    thinking_level::ThinkingSelectorState,
    ui_state::{AgentUiAction, AgentUiState},
};
use crate::{
    component::{
        agent::SurfaceNavigation,
        text_input::{TextInput, TextInputEvent},
    },
    file_search::FileEntry,
};

/// Compatibility façade for the interactive Agent feature.
///
/// Runtime behavior lives in [`AgentRuntime`]; interactive state lives in [`AgentUiState`].
#[derive(Debug, Default)]
pub struct AgentState {
    runtime: AgentRuntime,
    ui: AgentUiState,
}

impl AgentState {
    pub fn runtime(&self) -> &AgentRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut AgentRuntime {
        &mut self.runtime
    }

    pub(crate) fn ui(&self) -> &AgentUiState {
        &self.ui
    }

    pub(crate) fn ui_mut(&mut self) -> &mut AgentUiState {
        &mut self.ui
    }

    pub(crate) fn dispatch(&mut self, action: AgentUiAction) {
        self.ui.reduce(action);
    }

    pub fn prompt(&self) -> &str {
        self.ui.prompt()
    }

    pub fn prompt_state(&self) -> &TextInput {
        self.ui.prompt_state()
    }

    #[cfg(test)]
    pub fn set_prompt(&mut self, value: impl Into<String>) {
        self.ui.set_prompt(value);
    }

    pub fn command_state(&self) -> &PromptCommandState {
        self.ui.command_state()
    }

    pub fn quick_fill_prompt_command(&mut self, movement: PromptCommandMovement) -> bool {
        self.ui.quick_fill_prompt_command(movement)
    }

    pub fn file_references(&self) -> Option<FileReferenceView<'_>> {
        self.ui.file_references()
    }

    pub fn file_references_active(&self) -> bool {
        self.ui.file_references_active()
    }

    pub fn update_file_references(
        &mut self,
        root: &std::path::Path,
        current: Option<FileEntry>,
    ) -> Option<super::references::FileReferenceRequest> {
        self.ui.update_file_references(root, current)
    }

    pub fn select_next_file_reference(&mut self) {
        self.ui.select_next_file_reference();
    }

    pub fn select_previous_file_reference(&mut self) {
        self.ui.select_previous_file_reference();
    }

    pub fn dismiss_file_references(&mut self) {
        self.ui.dismiss_file_references();
    }

    pub fn accept_file_reference(&mut self) -> bool {
        self.ui.accept_file_reference()
    }

    pub(crate) fn complete_file_reference_scan(
        &mut self,
        revision: u64,
        result: Result<Vec<FileEntry>, String>,
    ) -> Option<super::references::FileReferenceRequest> {
        self.ui.complete_file_reference_scan(revision, result)
    }

    pub(crate) fn complete_file_reference_filter(
        &mut self,
        revision: u64,
        results: Vec<FileEntry>,
    ) {
        self.ui.complete_file_reference_filter(revision, results);
    }

    pub fn panel_mode(&self) -> AgentInputMode {
        self.ui.panel_mode()
    }

    pub fn panel_input_context(&self) -> crate::data_object::Context {
        self.ui.panel_input_context()
    }

    pub fn navigation(&self, surface: AgentSurface) -> SurfaceNavigation {
        self.ui.navigation(surface)
    }

    pub fn help_visible(&self) -> bool {
        self.ui.help_visible()
    }

    pub fn dialog_state(&self) -> AgentDialogState {
        self.ui.dialog_state()
    }

    /// Whether an inline workflow exists, including while temporarily suspended.
    pub fn dialog_open(&self) -> bool {
        self.ui.dialog_open()
    }

    pub fn thinking_selector(&self) -> &ThinkingSelectorState {
        self.ui.thinking_selector()
    }

    pub fn session_selector(&self) -> &SessionSelectorState {
        self.ui.session_selector()
    }

    pub fn context(&self) -> Option<&EditorContext> {
        self.ui.context()
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        self.ui.handle_input_event(event);
    }

    pub fn backspace(&mut self) {
        self.ui.backspace();
    }

    pub fn complete_prompt_command(&mut self) {
        self.ui.complete_prompt_command();
    }

    pub fn session(&self) -> &AgentSession {
        self.runtime.session()
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        self.runtime.session_mut()
    }

    pub fn set_provider(&mut self, provider: Arc<dyn AgentProvider>) {
        self.runtime.set_provider(provider);
    }

    pub fn set_provider_metadata(&mut self, name: String, model: Option<String>) {
        self.runtime.set_provider_metadata(name, model);
    }

    pub fn clear_provider(&mut self) {
        self.runtime.clear_provider();
    }

    pub fn provider(&self) -> Option<Arc<dyn AgentProvider>> {
        self.runtime.provider()
    }

    pub fn provider_name(&self) -> &str {
        self.runtime.provider_name()
    }

    pub fn model(&self) -> Option<&str> {
        self.runtime.model()
    }

    pub fn set_session_store(&mut self, store: Arc<dyn SessionStore>) {
        self.runtime.set_session_store(store);
    }

    pub(crate) fn storage_error(&self) -> Option<&str> {
        self.runtime.storage_error()
    }

    pub fn begin_cancellation(&mut self) -> CancellationToken {
        self.runtime.begin_cancellation()
    }

    pub(crate) fn take_suspension(&mut self, run_id: RunId) -> Option<AgentContinuation> {
        self.runtime.take_suspension(run_id)
    }
}
