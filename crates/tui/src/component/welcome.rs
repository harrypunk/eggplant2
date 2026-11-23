use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{
    data_object::{CommandId, FocusState},
    theme::UiTheme,
};

const LOGO: &str = r#"  ___  __ _  __ _  _ __  | |  __ _  _ __  | |_
 / _ \/ _` |/ _` || '_ \ | | / _` || '_ \ | __|
|  __/ (_| | (_| || |_) || || (_| || | | || |_
 \___|\__, |\__, || .__/ |_| \__,_||_| |_| \__|
      |___/ |___/ |_|"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeCommand {
    pub label: &'static str,
    pub key: &'static str,
    command: CommandId,
}

impl WelcomeCommand {
    pub fn command(self) -> CommandId {
        self.command
    }
}

pub const COMMANDS: [WelcomeCommand; 5] = [
    WelcomeCommand {
        label: "Find a file",
        key: "Space Space",
        command: CommandId("open_file_picker"),
    },
    WelcomeCommand {
        label: "Toggle explorer",
        key: "Space e",
        command: CommandId("toggle_file_explorer"),
    },
    WelcomeCommand {
        label: "Agent panel",
        key: "Space a t",
        command: CommandId("agent_toggle_panel"),
    },
    WelcomeCommand {
        label: "Choose theme",
        key: "Space u t",
        command: CommandId("open_theme_picker"),
    },
    WelcomeCommand {
        label: "Quit",
        key: "Space q q",
        command: CommandId("quit"),
    },
];

/// Selection for the read-only empty-editor command menu.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeState {
    selected: usize,
}

impl WelcomeState {
    pub fn selected(self) -> usize {
        self.selected
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1) % COMMANDS.len();
    }

    pub fn select_previous(&mut self) {
        self.selected = (self.selected + COMMANDS.len() - 1) % COMMANDS.len();
    }

    pub fn selected_command(self) -> CommandId {
        COMMANDS[self.selected].command()
    }
}

/// Empty-editor content with command buttons; this does not create a buffer.
pub struct Welcome<'a> {
    styles: &'a UiTheme,
    state: WelcomeState,
    focus: FocusState,
}

impl<'a> Welcome<'a> {
    pub fn new(styles: &'a UiTheme, state: WelcomeState, focus: FocusState) -> Self {
        Self {
            styles,
            state,
            focus,
        }
    }

    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let logo_width = LOGO
            .lines()
            .map(|line| Line::raw(line).width())
            .max()
            .unwrap_or(0) as u16;
        let mut lines = if width >= logo_width {
            LOGO.lines()
                .map(|line| Line::styled(line.to_string(), self.styles.action_hint))
                .collect::<Vec<_>>()
        } else {
            vec![Line::styled("eggplant", self.styles.action_hint)]
        };
        lines.extend([Line::default(), Line::default()]);
        lines.extend(COMMANDS.into_iter().enumerate().map(|(index, command)| {
            let style = if index == self.state.selected() {
                match self.focus {
                    FocusState::Focused => self.styles.base.patch(self.styles.selection),
                    FocusState::Unfocused => {
                        self.styles.base.patch(self.styles.unfocused_selection)
                    }
                }
            } else {
                self.styles.popup_surface()
            };
            let text = format!("  {:<18} {:>11}  ", command.label, command.key);
            Line::from(Span::styled(text, style))
        }));
        lines
    }
}

impl Widget for Welcome<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let lines = self.lines(area.width);
        let height = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .min(area.height);
        let [_, content, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .areas(area);
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(self.styles.base)
            .render(content, buffer);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Modifier};

    use super::*;
    use crate::theme::Theme;

    fn render(state: WelcomeState, focus: FocusState) -> (String, ratatui::buffer::Buffer) {
        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(70, 18)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(Welcome::new(theme.ui(), state, focus), frame.area());
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let content = buffer.content().iter().map(|cell| cell.symbol()).collect();
        (content, buffer)
    }

    #[test]
    fn shows_logo_and_command_buttons() {
        let (content, _) = render(WelcomeState::default(), FocusState::Focused);
        assert!(content.contains("|___/"));
        for command in COMMANDS {
            assert!(content.contains(command.label), "missing {}", command.label);
            assert!(content.contains(command.key), "missing {}", command.key);
        }
    }

    #[test]
    fn selected_button_distinguishes_focus() {
        let (_, focused) = render(WelcomeState::default(), FocusState::Focused);
        assert!(focused.content().iter().any(|cell| {
            cell.style().add_modifier.contains(Modifier::REVERSED)
                && !cell.style().add_modifier.contains(Modifier::DIM)
        }));

        let (_, unfocused) = render(WelcomeState::default(), FocusState::Unfocused);
        assert!(unfocused.content().iter().any(|cell| {
            cell.style().add_modifier.contains(Modifier::REVERSED)
                && cell.style().add_modifier.contains(Modifier::DIM)
        }));
    }

    #[test]
    fn navigation_wraps_and_exposes_the_selected_command() {
        let mut state = WelcomeState::default();
        assert_eq!(state.selected_command(), CommandId("open_file_picker"));
        state.select_previous();
        assert_eq!(state.selected_command(), CommandId("quit"));
        state.select_next();
        assert_eq!(state.selected_command(), CommandId("open_file_picker"));
        state.select_next();
        assert_eq!(state.selected_command(), CommandId("toggle_file_explorer"));
    }
}
