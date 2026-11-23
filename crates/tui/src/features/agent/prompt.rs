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
        self.input.handle_event(event);
        self.commands.clear();
        self.draft = DraftOrigin::User;
    }

    pub fn backspace(&mut self) {
        self.input.backspace();
        self.commands.clear();
        self.draft = DraftOrigin::User;
    }

    pub fn take_for_submission(&mut self) -> Option<String> {
        let prompt = self.input.trim().to_string();
        if prompt.is_empty() {
            return None;
        }
        self.input.clear();
        self.commands.clear();
        self.draft = DraftOrigin::Empty;
        Some(prompt)
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
    fn submission_clears_input_and_selection() {
        let mut prompt = AgentPrompt::default();
        prompt.prepare_dialog_prefill("main.rs:1".to_string());
        assert_eq!(prompt.take_for_submission().as_deref(), Some("main.rs:1"));
        assert_eq!(prompt.value(), "");
        assert!(prompt.take_for_submission().is_none());
    }
}
