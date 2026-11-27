use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    widgets::Widget,
};

use super::{
    command_menu::AgentCommandMenu,
    reference_menu::{AgentReferenceMenu, AgentReferenceProps},
};
use crate::{
    agent::PromptCommand,
    component::{text_input::TextInput, text_layout::TextFlow},
    theme::UiTheme,
};

/// Inputs for the shared Agent completion menus + text-input view.
#[derive(Clone)]
pub struct AgentPromptProps<'a> {
    input: &'a TextInput,
    commands: Option<Vec<PromptCommand>>,
    selected_command: Option<PromptCommand>,
    references: Option<AgentReferenceProps>,
}

impl<'a> AgentPromptProps<'a> {
    pub fn new(input: &'a TextInput) -> Self {
        Self {
            input,
            commands: None,
            selected_command: None,
            references: None,
        }
    }

    pub fn with_commands(
        mut self,
        commands: Option<Vec<PromptCommand>>,
        selected_command: Option<PromptCommand>,
    ) -> Self {
        self.commands = commands;
        self.selected_command = selected_command;
        self
    }

    pub fn input(&self) -> &TextInput {
        self.input
    }

    pub fn with_references(mut self, references: Option<AgentReferenceProps>) -> Self {
        self.references = references;
        self
    }

    pub fn references(&self) -> Option<&AgentReferenceProps> {
        self.references.as_ref()
    }

    pub fn commands(&self) -> Option<&[PromptCommand]> {
        self.commands.as_deref()
    }

    pub fn selected_command(&self) -> Option<PromptCommand> {
        self.selected_command
    }
}

pub struct AgentPromptView;

impl AgentPromptView {
    pub fn command_height(props: &AgentPromptProps<'_>) -> u16 {
        AgentCommandMenu::height(props.commands())
    }

    pub fn completion_height(props: &AgentPromptProps<'_>) -> u16 {
        Self::command_height(props).max(AgentReferenceMenu::height(props.references()))
    }

    pub fn layout(props: &AgentPromptProps<'_>, area: Rect, input: u16) -> [Rect; 2] {
        let completion = Self::completion_height(props).min(area.height.saturating_sub(input));
        Layout::vertical([Constraint::Length(completion), Constraint::Length(input)]).areas(area)
    }

    /// Render the shared prompt and return the exact text-input area for cursor policy.
    pub fn render(
        props: &AgentPromptProps<'_>,
        styles: &UiTheme,
        area: Rect,
        input_height: u16,
        buffer: &mut Buffer,
    ) -> Rect {
        let [completion, input] = Self::layout(props, area, input_height);
        if let Some(references) = props.references() {
            AgentReferenceMenu::render(references, styles, completion, buffer);
        } else if let Some(command_items) = props.commands() {
            AgentCommandMenu::render(
                command_items,
                props.selected_command(),
                styles,
                completion,
                buffer,
            );
        }
        props
            .input()
            .view_with_flow(styles, TextFlow::Wrapped)
            .render(input, buffer);
        input
    }

    pub fn cursor(props: &AgentPromptProps<'_>, styles: &UiTheme, input: Rect) -> Option<Position> {
        props
            .input()
            .cursor_position_with_flow(input, styles, TextFlow::Wrapped)
    }
}
