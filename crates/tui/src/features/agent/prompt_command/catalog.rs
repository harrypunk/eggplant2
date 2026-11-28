use std::sync::OnceLock;

use super::metadata::{CommandId, PromptCommand};
use crate::{data_object::Model, manager::effect::Effect};

pub type PromptCommandEffect = fn(&mut Model) -> Option<Effect>;

#[derive(Debug)]
pub struct PromptCommandDefinition {
    command: PromptCommand,
    effect: PromptCommandEffect,
}

impl PromptCommandDefinition {
    pub(crate) fn new(command: PromptCommand, effect: PromptCommandEffect) -> Self {
        Self { command, effect }
    }
}

#[derive(Debug)]
pub struct PromptCommandCatalog {
    definitions: Vec<PromptCommandDefinition>,
}

impl PromptCommandCatalog {
    pub(crate) fn new(definitions: Vec<PromptCommandDefinition>) -> Self {
        Self { definitions }
    }

    pub fn descriptors(&self) -> Vec<PromptCommand> {
        self.definitions
            .iter()
            .map(|definition| definition.command.clone())
            .collect()
    }

    pub fn filter(&self, prompt: &str) -> Option<Vec<PromptCommand>> {
        let query = prompt.strip_prefix('/')?.trim_end().to_ascii_lowercase();
        Some(
            self.definitions
                .iter()
                .filter(|definition| definition.command.matches(&query))
                .map(|definition| definition.command.clone())
                .collect(),
        )
    }

    pub fn resolve(&self, input: &str) -> Option<PromptCommand> {
        let input = input.trim();
        self.definitions
            .iter()
            .find(|definition| definition.command.name() == input)
            .map(|definition| definition.command.clone())
    }

    pub fn execute(&self, model: &mut Model, id: &CommandId) -> Option<Effect> {
        let definition = self
            .definitions
            .iter()
            .find(|definition| definition.command.id() == id)?;
        (definition.effect)(model)
    }
}

impl Default for PromptCommandCatalog {
    fn default() -> Self {
        Self::new(vec![
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "provider-login",
                    "/provider",
                    "Provider login",
                    "Authenticate a model provider",
                ),
                crate::provider_login::open,
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "select-model",
                    "/model",
                    "Select model",
                    "Choose an available provider model",
                ),
                crate::model_selector::open,
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "thinking-options",
                    "/thinking",
                    "Thinking settings",
                    "Select provider thinking options",
                ),
                super::super::thinking_level::open,
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "restore-session",
                    "/sessions",
                    "Restore session",
                    "Resume a saved workspace session",
                ),
                super::super::sessions::open,
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "new-session",
                    "/new",
                    "New session",
                    "Start a clean agent conversation",
                ),
                super::super::sessions::new_session,
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "provider-logout",
                    "/logout",
                    "Log out",
                    "Remove a stored provider credential",
                ),
                crate::provider_logout::open,
            ),
        ])
    }
}

pub fn shared() -> &'static PromptCommandCatalog {
    static CATALOG: OnceLock<PromptCommandCatalog> = OnceLock::new();
    CATALOG.get_or_init(PromptCommandCatalog::default)
}

pub fn catalog() -> Vec<PromptCommand> {
    shared().descriptors()
}

pub fn resolve(input: &str) -> Option<PromptCommand> {
    shared().resolve(input)
}

pub fn execute(model: &mut Model, command: &PromptCommand) -> Option<Effect> {
    shared().execute(model, command.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_effect(model: &mut Model) -> Option<Effect> {
        model.agent.set_prompt("custom executed");
        None
    }

    #[test]
    fn custom_definitions_compose_metadata_filtering_and_execution() {
        let catalog = PromptCommandCatalog::new(vec![PromptCommandDefinition::new(
            PromptCommand::new(
                "custom-action",
                "/custom",
                "Custom action",
                "Run a composed command",
            ),
            custom_effect,
        )]);
        let command = catalog.resolve("  /custom\t").unwrap();
        assert_eq!(command.title(), "Custom action");
        assert_eq!(command.description(), "Run a composed command");
        assert_eq!(catalog.filter("/cus").unwrap(), vec![command.clone()]);
        assert!(catalog.filter("/other").unwrap().is_empty());
        let mut model = Model::default();

        catalog.execute(&mut model, command.id());

        assert_eq!(model.agent.prompt(), "custom executed");
    }
}
