use std::collections::HashMap;

use color_eyre::eyre::eyre;

pub use crate::data_object::CommandId;
use crate::{
    data_object::{Message, Model},
    manager::effect::Effect,
};

pub type FeatureHandler = fn(&mut Model, CommandId) -> Option<Effect>;

#[derive(Debug, Clone)]
pub enum CommandHandler {
    Message(fn() -> Message),
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
    pub fn new(id: CommandId, description: &'static str, message: fn() -> Message) -> Self {
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
        let mut registry =
            Self::from_commands(builtin_commands()).expect("built-in command IDs must be unique");
        crate::buffer_jump::register_commands(&mut registry)
            .expect("buffer-jump commands must be unique");
        crate::buffer_grep::register_commands(&mut registry)
            .expect("buffer-grep commands must be unique");
        registry
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin_commands() -> impl IntoIterator<Item = Command> {
    [
        Command::feature(CommandId("quit"), "quit", crate::lifecycle::handle_command),
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
        Command::feature(
            CommandId("page_scroll_down"),
            "scroll down",
            crate::page_scroll::handle_command,
        ),
        Command::feature(
            CommandId("page_scroll_up"),
            "scroll up",
            crate::page_scroll::handle_command,
        ),
        Command::feature(
            CommandId("focus_left"),
            "focus left",
            crate::focus::handle_command,
        ),
        Command::feature(
            CommandId("focus_down"),
            "focus down",
            crate::focus::handle_command,
        ),
        Command::feature(
            CommandId("focus_up"),
            "focus up",
            crate::focus::handle_command,
        ),
        Command::feature(
            CommandId("focus_right"),
            "focus right",
            crate::focus::handle_command,
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
        Command::feature(
            CommandId("close_buffer"),
            "close buffer",
            crate::lifecycle::handle_command,
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
        Command::feature(
            CommandId("explorer_next"),
            "next entry",
            crate::explorer::handle_command,
        ),
        Command::feature(
            CommandId("explorer_previous"),
            "previous entry",
            crate::explorer::handle_command,
        ),
        Command::new(CommandId("open_selected"), "open selected", || {
            Message::OpenSelected
        }),
        Command::feature(
            CommandId("explorer_fold"),
            "fold folder",
            crate::explorer::handle_command,
        ),
        Command::feature(
            CommandId("explorer_unfold"),
            "unfold folder",
            crate::explorer::handle_command,
        ),
        Command::feature(
            CommandId("refresh_explorer"),
            "refresh explorer",
            crate::explorer::handle_command,
        ),
        Command::feature(
            CommandId("toggle_ignored_files"),
            "toggle ignored files",
            crate::explorer::handle_command,
        ),
        Command::feature(
            CommandId("toggle_file_explorer"),
            "toggle files",
            crate::focus::handle_command,
        ),
        Command::feature(
            CommandId("open_file_picker"),
            "find file",
            crate::file_picker::handle_command,
        ),
        Command::feature(
            CommandId("close_file_picker"),
            "close file picker",
            crate::file_picker::handle_command,
        ),
        Command::feature(
            CommandId("file_picker_backspace"),
            "delete query character",
            crate::file_picker::handle_command,
        ),
        Command::feature(
            CommandId("open_picker_file"),
            "open file",
            crate::file_picker::handle_command,
        ),
        Command::feature(
            CommandId("file_picker_next"),
            "next file",
            crate::file_picker::handle_command,
        ),
        Command::feature(
            CommandId("file_picker_previous"),
            "previous file",
            crate::file_picker::handle_command,
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
            Command::new(CommandId("test.command"), "first", || Message::FocusLost),
            Command::new(CommandId("test.command"), "second", || {
                Message::Resize(1, 1)
            }),
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
