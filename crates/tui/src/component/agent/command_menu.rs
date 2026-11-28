use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{agent::PromptCommand, component::picker, theme::UiTheme};

pub struct AgentCommandMenu;

impl AgentCommandMenu {
    /// Intrinsic content height; parent layouts bound the actual viewport to available space.
    pub fn height(commands: Option<&[PromptCommand]>) -> u16 {
        commands.map_or(0, |commands| {
            u16::try_from(commands.len().max(1)).unwrap_or(u16::MAX)
        })
    }

    pub fn render(
        commands: &[PromptCommand],
        selected: Option<PromptCommand>,
        styles: &UiTheme,
        area: Rect,
        buffer: &mut Buffer,
    ) {
        let lines = if commands.is_empty() {
            vec![Line::styled("No matching commands", styles.muted)]
        } else {
            commands
                .iter()
                .map(|command| command_line(*command, selected == Some(*command), styles))
                .collect()
        };
        let selected =
            selected.and_then(|selected| commands.iter().position(|command| *command == selected));
        let lines = picker::visible_lines(lines, selected, area.height);
        Paragraph::new(lines)
            .style(styles.popup_surface())
            .render(area, buffer);
    }
}

fn command_line<'a>(command: PromptCommand, selected: bool, styles: &UiTheme) -> Line<'a> {
    let style = if selected {
        styles.base.patch(styles.selection)
    } else {
        styles.popup_surface()
    };
    Line::from(vec![
        Span::styled(format!("{:<16}", command.name()), style),
        Span::styled(command.description(), style),
    ])
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Modifier};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_only_commands_and_an_empty_filter_state() {
        let theme = Theme::default();
        let commands = [PromptCommand::ProviderLogin];
        let mut terminal = Terminal::new(TestBackend::new(32, 2)).unwrap();
        terminal
            .draw(|frame| {
                AgentCommandMenu::render(
                    &commands,
                    None,
                    theme.ui(),
                    frame.area(),
                    frame.buffer_mut(),
                )
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("/provider"));
        assert!(!content.contains("Agent commands"));
        assert!(content.contains("Provider login"));
        assert!(!content.contains("Enter run"));

        let mut empty = Terminal::new(TestBackend::new(32, 2)).unwrap();
        empty
            .draw(|frame| {
                AgentCommandMenu::render(&[], None, theme.ui(), frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let content = empty
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("No matching commands"));
    }

    #[test]
    fn selection_is_highlighted_only_after_quick_fill() {
        let theme = Theme::default();
        let commands = [PromptCommand::ProviderLogin];
        let mut terminal = Terminal::new(TestBackend::new(32, 1)).unwrap();
        terminal
            .draw(|frame| {
                AgentCommandMenu::render(
                    &commands,
                    None,
                    theme.ui(),
                    frame.area(),
                    frame.buffer_mut(),
                )
            })
            .unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| !cell.style().add_modifier.contains(Modifier::REVERSED))
        );

        terminal
            .draw(|frame| {
                AgentCommandMenu::render(
                    &commands,
                    Some(PromptCommand::ProviderLogin),
                    theme.ui(),
                    frame.area(),
                    frame.buffer_mut(),
                )
            })
            .unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .any(|cell| cell.style().add_modifier.contains(Modifier::REVERSED))
        );
    }

    #[test]
    fn bounded_viewport_keeps_the_selected_command_visible() {
        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(32, 1)).unwrap();
        terminal
            .draw(|frame| {
                AgentCommandMenu::render(
                    &PromptCommand::ALL,
                    Some(PromptCommand::Logout),
                    theme.ui(),
                    frame.area(),
                    frame.buffer_mut(),
                )
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("/logout"));
        assert!(!content.contains("/provider"));
    }

    #[test]
    fn height_is_derived_from_menu_visibility_and_rows() {
        assert_eq!(AgentCommandMenu::height(None), 0);
        assert_eq!(AgentCommandMenu::height(Some(&[])), 1);
        assert_eq!(
            AgentCommandMenu::height(Some(&[PromptCommand::ProviderLogin])),
            1
        );
        assert_eq!(
            AgentCommandMenu::height(Some(&PromptCommand::ALL)),
            u16::try_from(PromptCommand::ALL.len()).unwrap()
        );
    }
}
