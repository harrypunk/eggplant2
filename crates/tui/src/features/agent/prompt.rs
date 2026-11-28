use crate::component::text_input::{TextInput, TextInputEvent};

use super::{
    PromptCommandMovement, PromptCommandState,
    references::{FileReferenceRequest, FileReferenceState, FileReferenceView},
};
use crate::file_search::FileEntry;

/// Prompt input with independently composable slash-command and file-reference completion.
#[derive(Debug, Default)]
pub struct AgentPrompt {
    input: TextInput,
    commands: PromptCommandState,
    references: FileReferenceState,
}

impl AgentPrompt {
    pub fn value(&self) -> &str {
        self.input.value()
    }

    pub fn input(&self) -> &TextInput {
        &self.input
    }

    #[cfg(test)]
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.input.set_value(value);
        self.commands.clear();
        self.references.clear();
    }

    pub fn command_state(&self) -> &PromptCommandState {
        &self.commands
    }

    pub fn reference_view(&self) -> Option<FileReferenceView<'_>> {
        self.references.view()
    }

    pub fn references_active(&self) -> bool {
        self.references.is_active()
    }

    pub fn update_references(
        &mut self,
        root: &std::path::Path,
        current: Option<FileEntry>,
    ) -> Option<FileReferenceRequest> {
        self.references.update(&self.input, root, current)
    }

    pub fn select_next_reference(&mut self) {
        self.references.select_next();
    }

    pub fn select_previous_reference(&mut self) {
        self.references.select_previous();
    }

    pub fn dismiss_references(&mut self) {
        self.references.dismiss();
    }

    pub fn accept_reference(&mut self) -> bool {
        let Some(acceptance) = self.references.acceptance(&self.input) else {
            return false;
        };
        if !self
            .input
            .replace_range(acceptance.range, &acceptance.replacement)
        {
            return false;
        }
        self.references.clear();
        self.commands.clear();
        true
    }

    pub(super) fn complete_reference_scan(
        &mut self,
        revision: u64,
        result: Result<Vec<FileEntry>, String>,
    ) -> Option<FileReferenceRequest> {
        self.references.complete_scan(revision, result)
    }

    pub(super) fn complete_reference_filter(&mut self, revision: u64, results: Vec<FileEntry>) {
        self.references.complete_filter(revision, results);
    }

    pub fn quick_fill(&mut self, movement: PromptCommandMovement) -> bool {
        let Some(name) = self.commands.quick_fill(self.input.value(), movement) else {
            return false;
        };
        self.input.set_value(name);
        true
    }

    pub fn edit(&mut self, event: TextInputEvent) {
        if self.input.handle_event(event).content_changed() {
            self.commands.clear();
        }
    }

    pub fn backspace(&mut self) {
        if self.input.backspace() {
            self.commands.clear();
        }
    }

    #[cfg(test)]
    pub fn submission_text(&self) -> Option<String> {
        let prompt = self.input.trim().to_string();
        (!prompt.is_empty()).then_some(prompt)
    }

    pub fn complete_submission(&mut self) {
        self.input.clear();
        self.commands.clear();
        self.references.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_movement_and_noop_backspace_preserve_prompt_policy() {
        let mut prompt = AgentPrompt::default();
        prompt.backspace();
        assert_eq!(prompt.value(), "");

        prompt.set_value("/");
        assert!(prompt.quick_fill(PromptCommandMovement::Next));
        prompt.edit(TextInputEvent::Cursor(
            crate::component::text_input::TextCursorMovement::Left,
        ));
        assert!(prompt.command_state().selected().is_some());
    }

    #[test]
    fn submission_clears_only_when_completed() {
        let mut prompt = AgentPrompt::default();
        prompt.set_value("explain @main.rs");
        assert_eq!(
            prompt.submission_text().as_deref(),
            Some("explain @main.rs")
        );
        assert_eq!(prompt.value(), "explain @main.rs");
        prompt.complete_submission();
        assert_eq!(prompt.value(), "");
        assert!(prompt.submission_text().is_none());
    }
}
