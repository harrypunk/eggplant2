use std::collections::HashMap;

use color_eyre::eyre::eyre;
use editor::DocumentFactory;

pub use crate::data_object::CommandId;
use crate::{
    data_object::{Message, Model},
    manager::effect::Effect,
    theme_picker::ThemePickerCommand,
};

pub type FeatureHandler = fn(&mut Model) -> Option<Effect>;
pub type DocumentHandler = fn(&DocumentFactory, &mut Model) -> Option<Effect>;
pub type CommandSelector = fn(&Model) -> Option<CommandId>;
/// A feature's command metadata; the registry owns insertion and duplicate validation.
type CommandProvider = fn() -> Vec<Command>;

#[derive(Debug, Clone)]
pub enum CommandHandler {
    Message(fn() -> Message),
    Feature(FeatureHandler),
    /// Invoke a command that needs the shared document-opening capability.
    Document(DocumentHandler),
    /// Resolve a data-driven command target, then use the normal dispatch path.
    Dispatch(CommandSelector),
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

    pub fn document(id: CommandId, description: &'static str, handler: DocumentHandler) -> Self {
        Self {
            id,
            description,
            handler: CommandHandler::Document(handler),
        }
    }

    pub fn dispatch(id: CommandId, description: &'static str, selector: CommandSelector) -> Self {
        Self {
            id,
            description,
            handler: CommandHandler::Dispatch(selector),
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
        const PROVIDERS: &[CommandProvider] = &[
            crate::buffer_jump::commands,
            crate::buffer_grep::commands,
            crate::buffer_navigation::commands,
            crate::project_grep::commands,
            crate::line_jump::commands,
            crate::view_align::commands,
            crate::cursor_line::commands,
            crate::change::commands,
            crate::yank::commands,
            crate::vcs::commands,
            crate::code_navigation::commands,
            crate::diagnostics::commands,
            crate::agent::commands,
            crate::provider_login::commands,
            crate::provider_logout::commands,
            crate::model_selector::commands,
            crate::welcome::commands,
        ];

        let feature_commands = PROVIDERS.iter().flat_map(|provider| provider());
        Self::from_commands(builtin_commands().into_iter().chain(feature_commands))
            .expect("built-in command IDs must be unique")
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin_commands() -> impl IntoIterator<Item = Command> {
    [
        Command::feature(CommandId("quit"), "quit", crate::lifecycle::quit),
        Command::feature(
            CommandId("enter_visual_mode"),
            "enter Visual mode",
            crate::editing::enter_visual_mode,
        ),
        Command::feature(
            CommandId("enter_visual_line_mode"),
            "enter linewise Visual mode",
            crate::editing::enter_visual_line_mode,
        ),
        Command::feature(
            CommandId("exit_visual_mode"),
            "leave Visual mode",
            crate::editing::exit_visual_mode,
        ),
        Command::feature(
            CommandId("enter_insert_mode"),
            "enter Insert mode",
            crate::editing::enter_insert_mode,
        ),
        Command::feature(
            CommandId("insert_after_cursor"),
            "insert after cursor",
            crate::editing::insert_after_cursor,
        ),
        Command::feature(
            CommandId("insert_at_line_end"),
            "insert at line end",
            crate::editing::insert_at_line_end,
        ),
        Command::feature(
            CommandId("insert_at_first_nonblank"),
            "insert at first non-blank character",
            crate::editing::insert_at_first_nonblank,
        ),
        Command::feature(
            CommandId("open_line_below"),
            "open indented line below",
            crate::editing::open_line_below,
        ),
        Command::feature(
            CommandId("open_line_above"),
            "open indented line above",
            crate::editing::open_line_above,
        ),
        Command::feature(
            CommandId("exit_insert_mode"),
            "leave Insert mode",
            crate::editing::exit_insert_mode,
        ),
        Command::feature(
            CommandId("move_left"),
            "move left",
            crate::editing::move_left,
        ),
        Command::feature(
            CommandId("move_right"),
            "move right",
            crate::editing::move_right,
        ),
        Command::feature(CommandId("move_up"), "move up", crate::editing::move_up),
        Command::feature(
            CommandId("move_down"),
            "move down",
            crate::editing::move_down,
        ),
        Command::feature(
            CommandId("move_line_start"),
            "move to line start",
            crate::editing::move_line_start,
        ),
        Command::feature(
            CommandId("move_line_end"),
            "move to line end",
            crate::editing::move_line_end,
        ),
        Command::feature(
            CommandId("move_next_word_start"),
            "move to next word start",
            crate::editing::move_next_word_start,
        ),
        Command::feature(
            CommandId("move_previous_word_start"),
            "move to previous word start",
            crate::editing::move_previous_word_start,
        ),
        Command::feature(
            CommandId("move_next_word_end"),
            "move to next word end",
            crate::editing::move_next_word_end,
        ),
        Command::feature(
            CommandId("page_scroll_down"),
            "scroll down",
            crate::page_scroll::down,
        ),
        Command::feature(
            CommandId("page_scroll_up"),
            "scroll up",
            crate::page_scroll::up,
        ),
        Command::feature(
            CommandId("focus_left"),
            "focus left",
            crate::focus::focus_left,
        ),
        Command::feature(
            CommandId("focus_down"),
            "focus down",
            crate::focus::focus_down,
        ),
        Command::feature(CommandId("focus_up"), "focus up", crate::focus::focus_up),
        Command::feature(
            CommandId("focus_right"),
            "focus right",
            crate::focus::focus_right,
        ),
        Command::feature(
            CommandId("backspace"),
            "backspace",
            crate::editing::backspace,
        ),
        Command::feature(
            CommandId("insert_newline"),
            "insert newline",
            crate::editing::insert_newline,
        ),
        Command::feature(CommandId("undo"), "undo", crate::editing::undo),
        Command::feature(CommandId("redo"), "redo", crate::editing::redo),
        Command::feature(
            CommandId("close_buffer"),
            "close buffer",
            crate::lifecycle::close_buffer,
        ),
        Command::feature(
            CommandId("close_other_buffers"),
            "close other buffers",
            crate::lifecycle::close_other_buffers,
        ),
        Command::feature(
            CommandId("close_buffers_right"),
            "close buffers to the right",
            crate::lifecycle::close_buffers_right,
        ),
        Command::feature(
            CommandId("close_all_buffers"),
            "close all buffers",
            crate::lifecycle::close_all_buffers,
        ),
        Command::feature(CommandId("next_buffer"), "next buffer", crate::buffer::next),
        Command::feature(
            CommandId("previous_buffer"),
            "previous buffer",
            crate::buffer::previous,
        ),
        Command::feature(
            CommandId("explorer_next"),
            "next entry",
            crate::explorer::next,
        ),
        Command::feature(
            CommandId("explorer_previous"),
            "previous entry",
            crate::explorer::previous,
        ),
        Command::document(
            CommandId("open_selected"),
            "open selected",
            crate::explorer::open_selected,
        ),
        Command::feature(
            CommandId("explorer_fold"),
            "fold folder",
            crate::explorer::fold,
        ),
        Command::feature(
            CommandId("explorer_unfold"),
            "unfold folder",
            crate::explorer::unfold,
        ),
        Command::feature(
            CommandId("refresh_explorer"),
            "refresh explorer",
            crate::explorer::refresh,
        ),
        Command::feature(
            CommandId("toggle_ignored_files"),
            "toggle ignored files",
            crate::explorer::toggle_ignored_files,
        ),
        Command::feature(
            CommandId("explorer_help"),
            "show explorer help",
            crate::explorer::toggle_help,
        ),
        Command::feature(
            CommandId("close_explorer_help"),
            "close explorer help",
            crate::explorer::close_help,
        ),
        Command::feature(
            CommandId("explorer_add"),
            "add file or directory",
            crate::explorer::open_add,
        ),
        Command::feature(
            CommandId("explorer_add_submit"),
            "create file or directory",
            crate::explorer::submit_add,
        ),
        Command::feature(
            CommandId("close_explorer_add"),
            "cancel adding file or directory",
            crate::explorer::cancel_add,
        ),
        Command::feature(
            CommandId("explorer_delete"),
            "delete file or directory",
            crate::explorer::open_delete,
        ),
        Command::feature(
            CommandId("explorer_delete_confirm"),
            "confirm deletion",
            crate::explorer::confirm_delete,
        ),
        Command::feature(
            CommandId("close_explorer_delete"),
            "cancel deletion",
            crate::explorer::cancel_delete,
        ),
        Command::feature(
            CommandId("explorer_move"),
            "move file or directory",
            crate::explorer::open_move,
        ),
        Command::feature(
            CommandId("explorer_transfer_submit"),
            "confirm transfer destination",
            crate::explorer::submit_transfer,
        ),
        Command::feature(
            CommandId("close_explorer_transfer"),
            "cancel transfer",
            crate::explorer::cancel_transfer,
        ),
        Command::feature(
            CommandId("explorer_copy"),
            "copy file or directory",
            crate::explorer::open_copy,
        ),
        Command::feature(
            CommandId("toggle_file_explorer"),
            "toggle files",
            crate::focus::toggle_explorer,
        ),
        Command::feature(
            CommandId("open_file_picker"),
            "find file",
            crate::file_picker::open,
        ),
        Command::feature(
            CommandId("close_file_picker"),
            "close file picker",
            crate::file_picker::close_file_picker,
        ),
        Command::feature(
            CommandId("open_picker_file"),
            "open file",
            crate::file_picker::open_selected,
        ),
        Command::feature(
            CommandId("file_picker_next"),
            "next file",
            crate::file_picker::next,
        ),
        Command::feature(
            CommandId("file_picker_previous"),
            "previous file",
            crate::file_picker::previous,
        ),
        Command::feature(
            ThemePickerCommand::Open.id(),
            "theme",
            crate::theme_picker::open,
        ),
        Command::feature(
            ThemePickerCommand::Close.id(),
            "close theme picker",
            crate::theme_picker::close,
        ),
        Command::feature(
            ThemePickerCommand::Next.id(),
            "next theme",
            crate::theme_picker::next,
        ),
        Command::feature(
            ThemePickerCommand::Previous.id(),
            "previous theme",
            crate::theme_picker::previous,
        ),
        Command::feature(
            ThemePickerCommand::Apply.id(),
            "apply theme",
            crate::theme_picker::apply,
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
