mod catalog;
mod metadata;
mod state;

#[cfg(test)]
pub(crate) use catalog::resolve;
pub use catalog::{
    PromptCommandArguments, PromptCommandCatalog, PromptCommandDefinition, PromptCommandEffect,
    PromptCommandError, catalog,
};
pub(crate) use catalog::{execute, submission};
pub use metadata::{CommandId, PromptCommand, PromptCommandInvocation};
pub(crate) use state::{PromptCommandMovement, PromptCommandState};
