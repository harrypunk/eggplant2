use crate::{
    component::{
        agent::{PageScrollState, PanelNavigation, SurfaceNavigation},
        text_input::{TextInput, TextInputEvent},
    },
    file_search::FileEntry,
};

use super::{
    AgentDialogState, AgentInputMode, EditorContext, PromptCommandMovement, PromptCommandState,
    prompt::AgentPrompt, references, surface::AgentSurface,
};

/// Interactive-only Agent state. Runtime execution never depends on this type.
#[derive(Debug, Default)]
pub(crate) struct AgentUiState {
    prompt: AgentPrompt,
    dialog: AgentDialogState,
    panel_mode: AgentInputMode,
    dialog_scroll: PageScrollState,
    panel_navigation: PanelNavigation,
    help_visible: bool,
    context: Option<EditorContext>,
}

impl AgentUiState {
    pub fn prompt(&self) -> &str {
        self.prompt.value()
    }

    pub fn submission_text(&self) -> Option<String> {
        self.prompt.submission_text()
    }

    pub fn prompt_state(&self) -> &TextInput {
        self.prompt.input()
    }

    #[cfg(test)]
    pub fn set_prompt(&mut self, value: impl Into<String>) {
        self.prompt.set_value(value);
    }

    pub fn command_state(&self) -> &PromptCommandState {
        self.prompt.command_state()
    }

    pub fn quick_fill_prompt_command(&mut self, movement: PromptCommandMovement) -> bool {
        self.prompt.quick_fill(movement)
    }

    pub fn file_references(&self) -> Option<references::FileReferenceView<'_>> {
        self.prompt.reference_view()
    }

    pub fn file_references_active(&self) -> bool {
        self.prompt.references_active()
    }

    pub fn update_file_references(
        &mut self,
        root: &std::path::Path,
        current: Option<FileEntry>,
    ) -> Option<references::FileReferenceRequest> {
        self.prompt.update_references(root, current)
    }

    pub fn select_next_file_reference(&mut self) {
        self.prompt.select_next_reference();
    }

    pub fn select_previous_file_reference(&mut self) {
        self.prompt.select_previous_reference();
    }

    pub fn dismiss_file_references(&mut self) {
        self.prompt.dismiss_references();
    }

    pub fn accept_file_reference(&mut self) -> bool {
        self.prompt.accept_reference()
    }

    pub(crate) fn complete_file_reference_scan(
        &mut self,
        revision: u64,
        result: Result<Vec<FileEntry>, String>,
    ) -> Option<references::FileReferenceRequest> {
        self.prompt.complete_reference_scan(revision, result)
    }

    pub(crate) fn complete_file_reference_filter(
        &mut self,
        revision: u64,
        results: Vec<FileEntry>,
    ) {
        self.prompt.complete_reference_filter(revision, results);
    }

    pub fn panel_mode(&self) -> AgentInputMode {
        self.panel_mode
    }

    pub fn panel_input_context(&self) -> crate::data_object::Context {
        match self.panel_mode {
            AgentInputMode::Normal => crate::data_object::Context::AgentChat,
            AgentInputMode::Insert => crate::data_object::Context::AgentChatInsert,
        }
    }

    pub fn navigation(&self, surface: AgentSurface) -> SurfaceNavigation {
        match surface {
            AgentSurface::Dialog => SurfaceNavigation::Dialog(self.dialog_scroll),
            AgentSurface::Panel => SurfaceNavigation::Panel(self.panel_navigation),
        }
    }

    pub fn dialog_scroll_mut(&mut self) -> &mut PageScrollState {
        &mut self.dialog_scroll
    }

    pub fn panel_navigation_mut(&mut self) -> &mut PanelNavigation {
        &mut self.panel_navigation
    }

    pub fn help_visible(&self) -> bool {
        self.help_visible
    }

    pub fn enter_insert_mode(&mut self) {
        self.panel_mode = AgentInputMode::Insert;
        self.help_visible = false;
    }

    pub fn enter_normal_mode(&mut self) {
        self.panel_mode = AgentInputMode::Normal;
        self.panel_navigation.enter_normal();
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    pub fn close_help(&mut self) {
        self.help_visible = false;
    }

    pub fn dialog_state(&self) -> AgentDialogState {
        self.dialog
    }

    pub fn dialog_open(&self) -> bool {
        self.dialog != AgentDialogState::Closed
    }

    pub fn suspend_dialog(&mut self) {
        if self.dialog == AgentDialogState::Open {
            self.dialog = AgentDialogState::Suspended;
        }
    }

    pub fn resume_dialog(&mut self) {
        if self.dialog == AgentDialogState::Suspended {
            self.dialog = AgentDialogState::Open;
        }
    }

    pub fn context(&self) -> Option<&EditorContext> {
        self.context.as_ref()
    }

    pub fn prepare_panel(&mut self) {
        self.context = None;
        self.panel_mode = AgentInputMode::Normal;
        self.panel_navigation.enter_normal();
        self.help_visible = false;
    }

    pub fn open_dialog(&mut self, context: Option<EditorContext>) {
        self.context = context;
        self.dialog_scroll.bottom();
        self.dialog = AgentDialogState::Open;
    }

    pub fn close_dialog(&mut self) {
        self.dialog = AgentDialogState::Closed;
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        self.prompt.edit(event);
    }

    pub fn backspace(&mut self) {
        self.prompt.backspace();
    }

    pub fn complete_successful_submission(&mut self) {
        self.dialog_scroll.bottom();
        self.panel_navigation.follow_bottom();
        self.prompt.complete_submission();
    }

    pub fn complete_prompt_command(&mut self) {
        self.prompt.complete_submission();
    }
}
