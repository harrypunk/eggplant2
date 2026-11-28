use std::sync::OnceLock;

use eggplant_agent::SessionId;

use super::metadata::{PromptCommand, PromptCommandInvocation};
use crate::{data_object::Model, manager::effect::Effect};

pub type PromptCommandEffect = fn(&mut Model, &PromptCommandInvocation) -> Option<Effect>;

#[derive(Debug, Clone)]
pub enum PromptCommandArguments {
    Forbidden,
    ExactlyOne { label: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCommandError {
    usage: String,
}

impl PromptCommandError {
    fn new(usage: impl Into<String>) -> Self {
        Self {
            usage: usage.into(),
        }
    }
}

impl std::fmt::Display for PromptCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.usage)
    }
}

#[derive(Debug)]
pub struct PromptCommandDefinition {
    command: PromptCommand,
    arguments: PromptCommandArguments,
    effect: PromptCommandEffect,
}

impl PromptCommandDefinition {
    pub(crate) fn new(command: PromptCommand, effect: PromptCommandEffect) -> Self {
        Self {
            command,
            arguments: PromptCommandArguments::Forbidden,
            effect,
        }
    }

    pub(crate) fn with_arguments(mut self, arguments: PromptCommandArguments) -> Self {
        self.arguments = arguments;
        self
    }

    fn invoke(&self, input: &str) -> Option<Result<PromptCommandInvocation, PromptCommandError>> {
        let arguments = self.arguments(input)?;
        match &self.arguments {
            PromptCommandArguments::Forbidden if arguments.is_empty() => {
                Some(Ok(PromptCommandInvocation::new(self.command.clone(), None)))
            }
            PromptCommandArguments::Forbidden => None,
            PromptCommandArguments::ExactlyOne { label } if arguments.is_empty() => Some(Err(
                PromptCommandError::new(format!("Usage: {} <{label}>", self.command.name())),
            )),
            PromptCommandArguments::ExactlyOne { label } => {
                let mut parts = arguments.split_whitespace();
                let argument = parts.next();
                match (argument, parts.next()) {
                    (Some(argument), None) => Some(Ok(PromptCommandInvocation::new(
                        self.command.clone(),
                        Some(argument.to_string()),
                    ))),
                    _ => Some(Err(PromptCommandError::new(format!(
                        "Usage: {} <{label}>",
                        self.command.name()
                    )))),
                }
            }
        }
    }

    fn arguments<'a>(&self, input: &'a str) -> Option<&'a str> {
        let name = self.command.name();
        input.strip_prefix(name).and_then(|remainder| {
            (remainder.is_empty() || remainder.starts_with(char::is_whitespace))
                .then_some(remainder.trim())
        })
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

    pub fn submission(
        &self,
        input: &str,
        selected: Option<PromptCommand>,
    ) -> Option<Result<PromptCommandInvocation, PromptCommandError>> {
        match selected {
            Some(command) => self
                .definitions
                .iter()
                .find(|definition| definition.command.id() == command.id())
                .and_then(|definition| definition.invoke(input)),
            None => self
                .definitions
                .iter()
                .filter(|definition| definition.arguments(input).is_some())
                .max_by_key(|definition| definition.command.name().len())
                .and_then(|definition| definition.invoke(input)),
        }
    }

    pub fn execute(
        &self,
        model: &mut Model,
        invocation: &PromptCommandInvocation,
    ) -> Option<Effect> {
        let definition = self
            .definitions
            .iter()
            .find(|definition| definition.command.id() == invocation.command().id())?;
        (definition.effect)(model, invocation)
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
                |model, _| crate::provider_login::open(model),
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "select-model",
                    "/model",
                    "Select model",
                    "Choose an available provider model",
                ),
                |model, _| crate::model_selector::open(model),
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "thinking-options",
                    "/thinking",
                    "Thinking settings",
                    "Select provider thinking options",
                ),
                |model, _| super::super::thinking_level::open(model),
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "restore-session",
                    "/sessions",
                    "Restore session",
                    "Resume a saved workspace session",
                ),
                |model, _| super::super::sessions::open(model),
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "restore-session-id",
                    "/restore",
                    "Restore by ID",
                    "Restore a session by explicit ID, including another workspace",
                ),
                |model, invocation| {
                    let id = invocation.argument()?;
                    super::super::sessions::restore_by_id(model, SessionId::from(id))
                },
            )
            .with_arguments(PromptCommandArguments::ExactlyOne {
                label: "session-id",
            }),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "new-session",
                    "/new",
                    "New session",
                    "Start a clean agent conversation",
                ),
                |model, _| super::super::sessions::new_session(model),
            ),
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "provider-logout",
                    "/logout",
                    "Log out",
                    "Remove a stored provider credential",
                ),
                |model, _| crate::provider_logout::open(model),
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

#[cfg(test)]
pub fn resolve(input: &str) -> Option<PromptCommand> {
    shared().resolve(input)
}

pub fn submission(
    input: &str,
    selected: Option<PromptCommand>,
) -> Option<Result<PromptCommandInvocation, PromptCommandError>> {
    shared().submission(input, selected)
}

pub fn execute(model: &mut Model, invocation: &PromptCommandInvocation) -> Option<Effect> {
    shared().execute(model, invocation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_effect(model: &mut Model, _invocation: &PromptCommandInvocation) -> Option<Effect> {
        model.agent.set_prompt("custom executed");
        None
    }

    #[test]
    fn custom_definitions_compose_metadata_arguments_and_execution() {
        let catalog = PromptCommandCatalog::new(vec![
            PromptCommandDefinition::new(
                PromptCommand::new(
                    "custom-action",
                    "/custom",
                    "Custom action",
                    "Run a composed command",
                ),
                custom_effect,
            )
            .with_arguments(PromptCommandArguments::ExactlyOne {
                label: "session-id",
            }),
        ]);
        let command = catalog.resolve("  /custom\t").unwrap();
        assert_eq!(command.title(), "Custom action");
        assert_eq!(command.description(), "Run a composed command");
        assert_eq!(catalog.filter("/cus").unwrap(), vec![command.clone()]);
        assert!(catalog.filter("/other").unwrap().is_empty());
        assert!(catalog.submission("/custom", None).unwrap().is_err());
        assert!(
            catalog
                .submission("/custom one two", None)
                .unwrap()
                .is_err()
        );
        let invocation = catalog
            .submission("/custom session-123", None)
            .unwrap()
            .unwrap();
        assert_eq!(invocation.argument(), Some("session-123"));
        let mut model = Model::default();

        catalog.execute(&mut model, &invocation);

        assert_eq!(model.agent.prompt(), "custom executed");
    }

    #[test]
    fn forbidden_arguments_leave_non_command_input_unchanged() {
        assert!(shared().submission("/model extra", None).is_none());
    }
}
