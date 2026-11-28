use eggplant_agent::RunId;

use crate::{
    capabilities::page_scroll::{PageScroll, ScrollDirection},
    component::{
        agent::{SurfaceNavigation, ViewportMetrics},
        text_input::{TextInput, TextInputEvent},
    },
    file_search::FileEntry,
};

use super::{
    AgentDialogState, AgentInputMode, EditorContext, PromptCommandMovement, PromptCommandState,
    presentation::{
        dialog::{DialogAction, DialogPresentationState},
        panel::{PanelAction, PanelPresentationState},
    },
    prompt::AgentPrompt,
    references,
    surface::AgentSurface,
    thinking_level::ThinkingSelectorState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentUiAction {
    OpenDialog {
        context: Option<EditorContext>,
    },
    PreparePanel,
    Dialog(DialogAction),
    Panel(PanelAction),
    RunStarted {
        surface: AgentSurface,
        run_id: RunId,
    },
    SessionChanged,
    Page {
        surface: AgentSurface,
        page: PageScroll,
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    },
    MoveCursor {
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    },
}

/// Shared interactive data plus independently reduced dialog/panel presentations.
#[derive(Debug, Default)]
pub(crate) struct AgentUiState {
    prompt: AgentPrompt,
    dialog: DialogPresentationState,
    panel: PanelPresentationState,
    thinking_selector: ThinkingSelectorState,
    context: Option<EditorContext>,
}

impl AgentUiState {
    pub fn reduce(&mut self, action: AgentUiAction) {
        match action {
            AgentUiAction::OpenDialog { context } => {
                self.context = context;
                self.dialog.reduce(DialogAction::Open);
            }
            AgentUiAction::PreparePanel => {
                self.context = None;
                self.panel.reduce(PanelAction::Prepare);
            }
            AgentUiAction::Dialog(action) => self.dialog.reduce(action),
            AgentUiAction::Panel(action) => self.panel.reduce(action),
            AgentUiAction::RunStarted { surface, run_id } => {
                self.prompt.complete_submission();
                match surface {
                    AgentSurface::Dialog => self.dialog.reduce(DialogAction::RunStarted(run_id)),
                    AgentSurface::Panel => self.panel.reduce(PanelAction::RunStarted(run_id)),
                }
            }
            AgentUiAction::SessionChanged => {
                self.prompt.complete_submission();
                self.dialog.reduce(DialogAction::SessionChanged);
                self.panel.reduce(PanelAction::SessionChanged);
            }
            AgentUiAction::Page {
                surface,
                page,
                direction,
                metrics,
            } => match surface {
                AgentSurface::Dialog => self.dialog.reduce(DialogAction::Page {
                    page,
                    direction,
                    metrics,
                }),
                AgentSurface::Panel => self.panel.reduce(PanelAction::Page {
                    page,
                    direction,
                    metrics,
                }),
            },
            AgentUiAction::MoveCursor { direction, metrics } => self
                .panel
                .reduce(PanelAction::MoveCursor { direction, metrics }),
        }
    }

    pub fn prompt(&self) -> &str {
        self.prompt.value()
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
        self.panel.mode()
    }

    pub fn panel_input_context(&self) -> crate::data_object::Context {
        self.panel.input_context()
    }

    pub fn navigation(&self, surface: AgentSurface) -> SurfaceNavigation {
        match surface {
            AgentSurface::Dialog => SurfaceNavigation::Dialog(self.dialog.scroll()),
            AgentSurface::Panel => SurfaceNavigation::Panel(self.panel.navigation()),
        }
    }

    pub fn help_visible(&self) -> bool {
        self.panel.help_visible()
    }

    pub fn dialog_state(&self) -> AgentDialogState {
        self.dialog.lifecycle()
    }

    pub fn thinking_selector(&self) -> &ThinkingSelectorState {
        &self.thinking_selector
    }

    pub fn thinking_selector_mut(&mut self) -> &mut ThinkingSelectorState {
        &mut self.thinking_selector
    }

    pub fn dialog_open(&self) -> bool {
        self.dialog.is_open()
    }

    pub fn context(&self) -> Option<&EditorContext> {
        self.context.as_ref()
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        self.prompt.edit(event);
    }

    pub fn backspace(&mut self) {
        self.prompt.backspace();
    }

    pub fn complete_prompt_command(&mut self) {
        self.prompt.complete_submission();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentRuntime;

    #[test]
    fn run_started_reduces_only_the_active_transcript() {
        let metrics = ViewportMetrics {
            content_rows: 100,
            viewport_rows: 20,
        };
        let mut state = AgentUiState::default();
        state.set_prompt("next");
        state.reduce(AgentUiAction::Page {
            surface: AgentSurface::Panel,
            page: PageScroll::default(),
            direction: ScrollDirection::Up,
            metrics,
        });
        let panel_before = state.navigation(AgentSurface::Panel);
        let run_id = AgentRuntime::default()
            .begin("next".to_string(), None, 123)
            .unwrap()
            .id;

        state.reduce(AgentUiAction::RunStarted {
            surface: AgentSurface::Dialog,
            run_id,
        });

        assert_eq!(state.prompt(), "");
        assert_eq!(state.navigation(AgentSurface::Panel), panel_before);
    }

    #[test]
    fn session_changed_clears_prompt_and_resets_both_transcript_views() {
        let metrics = ViewportMetrics {
            content_rows: 100,
            viewport_rows: 20,
        };
        let mut state = AgentUiState::default();
        state.set_prompt("draft");
        for surface in [AgentSurface::Dialog, AgentSurface::Panel] {
            state.reduce(AgentUiAction::Page {
                surface,
                page: PageScroll::default(),
                direction: ScrollDirection::Up,
                metrics,
            });
        }
        let expected = AgentUiState::default();

        state.reduce(AgentUiAction::SessionChanged);

        assert_eq!(state.prompt(), "");
        assert_eq!(
            state.navigation(AgentSurface::Dialog),
            expected.navigation(AgentSurface::Dialog)
        );
        assert_eq!(
            state.navigation(AgentSurface::Panel),
            expected.navigation(AgentSurface::Panel)
        );
    }
}
