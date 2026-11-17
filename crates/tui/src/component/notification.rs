use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    capabilities::notification::{Notification, NotificationKind},
    theme::Theme,
};

const MAX_WIDTH: u16 = 48;
const MAX_HEIGHT: u16 = 8;

pub struct NotificationPopup<'a> {
    notification: &'a Notification,
    theme: &'a Theme,
}

impl<'a> NotificationPopup<'a> {
    pub fn new(notification: &'a Notification, theme: &'a Theme) -> Self {
        Self {
            notification,
            theme,
        }
    }

    pub fn render(&self, frame: &mut Frame, frame_area: Rect) {
        if frame_area.width == 0 || frame_area.height == 0 {
            return;
        }
        let styles = self.theme.ui();
        let surface = styles.popup_surface();
        let accent = match self.notification.kind() {
            NotificationKind::Info => styles.action_hint,
            NotificationKind::Error => styles.error,
        };
        let content = Paragraph::new(self.notification.message().to_string())
            .wrap(Wrap { trim: false })
            .style(surface.patch(styles.base));
        let area = popup_area(frame_area, self.notification.message());
        let title = self
            .notification
            .title()
            .unwrap_or(match self.notification.kind() {
                NotificationKind::Info => "Info",
                NotificationKind::Error => "Error",
            });
        let block = Block::bordered()
            .title(Line::styled(title.to_string(), surface.patch(accent)))
            .border_style(surface.patch(accent))
            .style(surface);
        let inner = block.inner(area);

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        frame.render_widget(content, inner);
    }
}

fn popup_area(frame: Rect, message: &str) -> Rect {
    let width = frame.width.min(MAX_WIDTH);
    let text_width = width.saturating_sub(2).max(1) as usize;
    let lines = message
        .lines()
        .map(|line| {
            let width = UnicodeWidthStr::width(line);
            width.max(1).div_ceil(text_width)
        })
        .sum::<usize>()
        .max(1);
    let height = (lines as u16 + 2).clamp(3, MAX_HEIGHT).min(frame.height);
    let right_margin = u16::from(frame.width > width);
    let top_margin = u16::from(frame.height > height);
    Rect::new(
        frame.x + frame.width.saturating_sub(width + right_margin),
        frame.y + top_margin,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_at_the_top_right_with_source_title() {
        let theme = Theme::default();
        let notification = Notification::info("Yanked text").with_title("Yank");
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                NotificationPopup::new(&notification, &theme).render(frame, frame.area());
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Yank"));
        assert!(content.contains("Yanked text"));
        assert_eq!(terminal.backend().buffer()[(31, 1)].symbol(), "┌");
    }

    #[test]
    fn clamps_to_a_tiny_terminal() {
        let theme = Theme::default();
        let notification = Notification::error("a long error that must remain bounded");
        let backend = TestBackend::new(10, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                NotificationPopup::new(&notification, &theme).render(frame, frame.area());
            })
            .unwrap();
        assert_eq!(terminal.backend().buffer().area().width, 10);
    }
}
