use editor::EditMode;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{
    data_object::{StatusKind, StatusMessage},
    theme::UiTheme,
};

const VCS_PLACEHOLDER: &str = " -- ";
const CLOCK_WIDTH: u16 = 6;

#[derive(Debug, Clone, Copy)]
pub struct StatusBar<'a> {
    mode: EditMode,
    file_path: &'a str,
    time: &'a str,
    message: Option<&'a StatusMessage>,
    pending_input: Option<&'a str>,
    styles: &'a UiTheme,
}

impl<'a> StatusBar<'a> {
    pub fn new(
        mode: EditMode,
        file_path: &'a str,
        time: &'a str,
        message: Option<&'a StatusMessage>,
        pending_input: Option<&'a str>,
        styles: &'a UiTheme,
    ) -> Self {
        Self {
            mode,
            file_path,
            time,
            message,
            pending_input,
            styles,
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, self.styles.base);
        let (mode, mode_style) = match self.mode {
            EditMode::Normal => ("NORMAL", self.styles.status_normal),
            EditMode::Insert => ("INSERT", self.styles.status_insert),
            EditMode::Visual => ("VISUAL", self.styles.selection),
            EditMode::VisualLine => ("V-LINE", self.styles.selection),
        };
        let pending_width = self
            .pending_input
            .map_or(0, |pending| pending.len() as u16 + 1);
        let [mode_area, vcs_area, path_area, pending_area, time_area] = Layout::horizontal([
            Constraint::Length(mode.len() as u16 + 2),
            Constraint::Length(VCS_PLACEHOLDER.len() as u16),
            Constraint::Fill(1),
            Constraint::Length(pending_width),
            Constraint::Length(CLOCK_WIDTH),
        ])
        .flex(Flex::Start)
        .areas(area);

        Paragraph::new(Span::styled(
            format!(" {mode} "),
            self.styles
                .base
                .patch(mode_style)
                .add_modifier(Modifier::BOLD),
        ))
        .render(mode_area, buf);
        Paragraph::new(Span::styled(VCS_PLACEHOLDER, self.styles.muted)).render(vcs_area, buf);

        let mut middle = vec![Span::styled(
            format!(" {}", self.file_path),
            self.styles.muted,
        )];
        if let Some(message) = self.message {
            let style = match message.kind {
                StatusKind::Error => self.styles.base.patch(self.styles.error),
                StatusKind::Info => self.styles.base,
            };
            middle.push(Span::styled(format!("  {}", message.text), style));
        }
        Paragraph::new(Line::from(middle)).render(path_area, buf);
        if let Some(pending) = self.pending_input {
            Paragraph::new(pending)
                .alignment(Alignment::Right)
                .style(self.styles.base.patch(self.styles.key_hint))
                .render(pending_area, buf);
        }
        Paragraph::new(self.time)
            .alignment(Alignment::Right)
            .style(self.styles.base)
            .render(time_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_current_mode() {
        let theme = Theme::default();
        for mode in [
            EditMode::Normal,
            EditMode::Insert,
            EditMode::Visual,
            EditMode::VisualLine,
        ] {
            let expected = match mode {
                EditMode::Normal => " NORMAL ",
                EditMode::Insert => " INSERT ",
                EditMode::Visual => " VISUAL ",
                EditMode::VisualLine => " V-LINE ",
            };
            let backend = TestBackend::new(40, 1);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|frame| {
                    frame.render_widget(
                        StatusBar::new(mode, "", "14:05", None, None, theme.ui()),
                        frame.area(),
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
            assert!(content.starts_with(expected));
        }
    }

    #[test]
    fn path_grows_and_clock_stays_right_aligned() {
        let theme = Theme::default();
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusBar::new(
                        EditMode::Normal,
                        "src/main.rs",
                        "14:05",
                        None,
                        None,
                        theme.ui(),
                    ),
                    frame.area(),
                );
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(&content[0..8], " NORMAL ");
        assert_eq!(&content[8..12], VCS_PLACEHOLDER);
        assert!(content[12..54].starts_with(" src/main.rs"));
        assert_eq!(&content[54..60], " 14:05");
    }

    #[test]
    fn renders_transient_pending_input() {
        let theme = Theme::default();
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusBar::new(EditMode::Normal, "", "14:05", None, Some("12g"), theme.ui()),
                    frame.area(),
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
        assert_eq!(&content[20..30], " 12g 14:05");
    }

    #[test]
    fn renders_error_message() {
        let message = StatusMessage::error("read failed");
        let theme = Theme::default();
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusBar::new(
                        EditMode::Normal,
                        "",
                        "14:05",
                        Some(&message),
                        None,
                        theme.ui(),
                    ),
                    frame.area(),
                );
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("read failed"));
        assert!(content.ends_with(" 14:05"));
    }
}
