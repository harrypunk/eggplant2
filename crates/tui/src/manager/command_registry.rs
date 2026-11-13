use std::collections::HashMap;

use color_eyre::eyre::eyre;

pub use crate::data_object::CommandId;
use crate::{
    data_object::{FocusDirection, Message, Model},
    manager::effect::Effect,
};

pub type FeatureHandler = fn(&mut Model, CommandId) -> Option<Effect>;

#[derive(Debug, Clone)]
pub enum CommandHandler {
    Message(Message),
    Feature(FeatureHandler),
}

/// A registered command. Message resolution is the M6.1 compatibility boundary;
/// later slices replace this with narrower feature-owned handlers.
#[derive(Debug, Clone)]
pub struct Command {
    pub id: CommandId,
    pub description: &'static str,
    pub handler: CommandHandler,
}

impl Command {
    pub fn new(id: CommandId, description: &'static str, message: Message) -> Self {
        Self {
            id,
            description,
            handler: CommandHandler::Message(message),
        }
    }

    pub fn feature(id: CommandId, description: &'static str, handler: FeatureHandler) -> Self {
        Self {
            id,
            description,
            handler: CommandHandler::Feature(handler),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRegistry {
    commands: HashMap<CommandId, Command>,
}

impl CommandRegistry {
    pub fn empty() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn from_commands(commands: impl IntoIterator<Item = Command>) -> color_eyre::Result<Self> {
        commands
            .into_iter()
            .try_fold(Self::empty(), |mut registry, command| {
                registry.register(command)?;
                Ok(registry)
            })
    }

    pub fn register(&mut self, command: Command) -> color_eyre::Result<()> {
        let id = command.id;
        if self.commands.contains_key(&id) {
            return Err(eyre!("duplicate command `{}`", id.as_str()));
        }
        self.commands.insert(id, command);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.get(id)
    }

    pub fn builtin() -> Self {
        Self::from_commands(builtin_commands()).expect("built-in command IDs must be unique")
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin_commands() -> impl IntoIterator<Item = Command> {
    [
        Command::new(CommandId("quit"), "quit", Message::Quit),
        Command::feature(
            CommandId("enter_insert_mode"),
            "enter Insert mode",
            crate::editing::handle_command,
        ),
        Command::feature(
            CommandId("exit_insert_mode"),
            "leave Insert mode",
            crate::editing::handle_command,
        ),
        Command::feature(
            CommandId("move_left"),
            "move left",
            crate::editing::handle_command,
        ),
        Command::feature(
            CommandId("move_right"),
            "move right",
            crate::editing::handle_command,
        ),
        Command::feature(
            CommandId("move_up"),
            "move up",
            crate::editing::handle_command,
        ),
        Command::feature(
            CommandId("move_down"),
            "move down",
            crate::editing::handle_command,
        ),
        Command::new(
            CommandId("focus_left"),
            "focus left",
            Message::MoveFocus(FocusDirection::Left),
        ),
        Command::new(
            CommandId("focus_down"),
            "focus down",
            Message::MoveFocus(FocusDirection::Down),
        ),
        Command::new(
            CommandId("focus_up"),
            "focus up",
            Message::MoveFocus(FocusDirection::Up),
        ),
        Command::new(
            CommandId("focus_right"),
            "focus right",
            Message::MoveFocus(FocusDirection::Right),
        ),
        Command::feature(
            CommandId("backspace"),
            "backspace",
            crate::editing::handle_command,
        ),
        Command::feature(
            CommandId("insert_newline"),
            "insert newline",
            crate::editing::handle_command,
        ),
        Command::feature(CommandId("undo"), "undo", crate::editing::handle_command),
        Command::feature(CommandId("redo"), "redo", crate::editing::handle_command),
        Command::new(
            CommandId("close_buffer"),
            "close buffer",
            Message::CloseActive,
        ),
        Command::feature(
            CommandId("next_buffer"),
            "next buffer",
            crate::buffer::handle_command,
        ),
        Command::feature(
            CommandId("previous_buffer"),
            "previous buffer",
            crate::buffer::handle_command,
        ),
        Command::new(
            CommandId("explorer_next"),
            "next entry",
            Message::ExplorerNext,
        ),
        Command::new(
            CommandId("explorer_previous"),
            "previous entry",
            Message::ExplorerPrevious,
        ),
        Command::new(
            CommandId("open_selected"),
            "open selected",
            Message::OpenSelected,
        ),
        Command::new(
            CommandId("explorer_fold"),
            "fold folder",
            Message::ExplorerFold,
        ),
        Command::new(
            CommandId("explorer_unfold"),
            "unfold folder",
            Message::ExplorerUnfold,
        ),
        Command::new(
            CommandId("refresh_explorer"),
            "refresh explorer",
            Message::RefreshExplorer,
        ),
        Command::new(
            CommandId("toggle_ignored_files"),
            "toggle ignored files",
            Message::ToggleIgnoredFiles,
        ),
        Command::new(
            CommandId("toggle_file_explorer"),
            "toggle files",
            Message::ToggleFileExplorer,
        ),
        Command::new(
            CommandId("open_file_picker"),
            "find file",
            Message::OpenFilePicker,
        ),
        Command::new(
            CommandId("close_file_picker"),
            "close file picker",
            Message::CloseFilePicker,
        ),
        Command::new(
            CommandId("file_picker_backspace"),
            "delete query character",
            Message::FilePickerBackspace,
        ),
        Command::new(
            CommandId("open_picker_file"),
            "open file",
            Message::OpenPickerFile,
        ),
        Command::new(
            CommandId("file_picker_next"),
            "next file",
            Message::FilePickerNext,
        ),
        Command::new(
            CommandId("file_picker_previous"),
            "previous file",
            Message::FilePickerPrevious,
        ),
        Command::feature(
            CommandId("open_theme_picker"),
            "theme",
            crate::theme_picker::handle_command,
        ),
        Command::feature(
            CommandId("close_theme_picker"),
            "close theme picker",
            crate::theme_picker::handle_command,
        ),
        Command::feature(
            CommandId("theme_picker_next"),
            "next theme",
            crate::theme_picker::handle_command,
        ),
        Command::feature(
            CommandId("theme_picker_previous"),
            "previous theme",
            crate::theme_picker::handle_command,
        ),
        Command::feature(
            CommandId("apply_theme"),
            "apply theme",
            crate::theme_picker::handle_command,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_command_ids() {
        let error = CommandRegistry::from_commands([
            Command::new(CommandId("test.command"), "first", Message::Quit),
            Command::new(CommandId("test.command"), "second", Message::FocusLost),
        ])
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("duplicate command `test.command`")
        );
    }

    #[test]
    fn builtin_registry_contains_metadata_and_semantic_handlers() {
        let registry = CommandRegistry::builtin();
        let command = registry.get("open_theme_picker").unwrap();
        assert_eq!(command.description, "theme");
        assert!(matches!(command.handler, CommandHandler::Feature(_)));
        assert!(registry.get("missing.command").is_none());
    }
}
