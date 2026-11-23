use crate::component::text_input::{TextInput, TextInputEvent};

use super::{PromptCommandMovement, PromptCommandState};

/// Distinguishes automatic dialog prefill from text the user actually owns.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DraftOrigin {
    #[default]
    Empty,
    DialogPrefill,
    User,
}

/// Prompt input, command-menu state, and draft policy shared by Agent surfaces.
#[derive(Debug, Default)]
pub struct AgentPrompt {
    input: TextInput,
    commands: PromptCommandState,
    draft: DraftOrigin,
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
        self.draft = DraftOrigin::User;
    }

    pub fn command_state(&self) -> &PromptCommandState {
        &self.commands
    }

    pub fn prepare_dialog_prefill(&mut self, label: String) {
        if self.draft != DraftOrigin::Empty {
            return;
        }
        self.input.set_value(label);
        self.commands.clear();
        self.draft = DraftOrigin::DialogPrefill;
    }

    pub fn prepare_panel(&mut self) {
        if self.draft != DraftOrigin::DialogPrefill {
            return;
        }
        self.input.clear();
        self.commands.clear();
        self.draft = DraftOrigin::Empty;
    }

    pub fn quick_fill(&mut self, movement: PromptCommandMovement) -> bool {
        let Some(name) = self.commands.quick_fill(self.input.value(), movement) else {
            return false;
        };
        self.input.set_value(name);
        self.draft = DraftOrigin::User;
        true
    }

    pub fn edit(&mut self, event: TextInputEvent) {
        if self.input.handle_event(event).content_changed() {
            self.commands.clear();
            self.draft = DraftOrigin::User;
        }
    }

    pub fn backspace(&mut self) {
        if self.input.backspace() {
            self.commands.clear();
            self.draft = DraftOrigin::User;
        }
    }

    pub fn submission_text(&self) -> Option<String> {
        let prompt = self.input.trim().to_string();
        (!prompt.is_empty()).then_some(prompt)
    }

    pub fn complete_submission(&mut self) {
        self.input.clear();
        self.commands.clear();
        self.draft = DraftOrigin::Empty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_clears_only_untouched_dialog_prefill() {
        let mut prompt = AgentPrompt::default();
        prompt.prepare_dialog_prefill("main.rs:1".to_string());
        prompt.prepare_panel();
        assert_eq!(prompt.value(), "");

        prompt.prepare_dialog_prefill("main.rs:1".to_string());
        prompt.edit(TextInputEvent::Character('?'));
        prompt.prepare_panel();
        assert_eq!(prompt.value(), "main.rs:1?");
    }

    #[test]
    fn cursor_movement_and_noop_backspace_preserve_prompt_policy() {
        let mut prompt = AgentPrompt::default();
        prompt.prepare_dialog_prefill("main.rs:1".to_string());
        prompt.edit(TextInputEvent::Cursor(
            crate::component::text_input::TextCursorMovement::Left,
        ));
        prompt.prepare_panel();
        assert_eq!(prompt.value(), "");

        prompt.backspace();
        prompt.prepare_dialog_prefill("main.rs:1".to_string());
        assert_eq!(prompt.value(), "main.rs:1");

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
        prompt.prepare_dialog_prefill("main.rs:1".to_string());
        assert_eq!(prompt.submission_text().as_deref(), Some("main.rs:1"));
        assert_eq!(prompt.value(), "main.rs:1");
        prompt.complete_submission();
        assert_eq!(prompt.value(), "");
        assert!(prompt.submission_text().is_none());
    }
}
