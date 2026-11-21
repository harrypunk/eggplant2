use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
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
const STACK_GAP: u16 = 1;

pub struct NotificationStack<'a> {
    notifications: Vec<&'a Notification>,
    theme: &'a Theme,
}

impl<'a> NotificationStack<'a> {
    pub fn new(
        notifications: impl IntoIterator<Item = &'a Notification>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            notifications: notifications.into_iter().collect(),
            theme,
        }
    }

    pub fn render(&self, frame: &mut Frame, frame_area: Rect) {
        if frame_area.width == 0 || frame_area.height == 0 {
            return;
        }
        let width = frame_area.width.min(MAX_WIDTH);
        let top_margin = u16::from(frame_area.height > 1);
        let heights = visible_heights(
            &self.notifications,
            width,
            frame_area.height.saturating_sub(top_margin),
        );
        if heights.is_empty() {
            return;
        }

        let total_height =
            heights.iter().sum::<u16>() + STACK_GAP * heights.len().saturating_sub(1) as u16;
        let right_margin = u16::from(frame_area.width > width);
        let stack_area = Rect::new(
            frame_area.x + frame_area.width.saturating_sub(width + right_margin),
            frame_area.y + top_margin,
            width,
            total_height,
        );
        let areas = Layout::vertical(heights.iter().copied().map(Constraint::Length))
            .flex(Flex::Start)
            .spacing(STACK_GAP)
            .split(stack_area);

        frame.render_widget(Clear, stack_area);
        for (notification, area) in self.notifications.iter().zip(areas.iter()) {
            NotificationPopup::new(notification, self.theme).render_at(frame, *area);
        }
    }
}

fn visible_heights(notifications: &[&Notification], width: u16, available: u16) -> Vec<u16> {
    let mut remaining = available;
    notifications
        .iter()
        .enumerate()
        .map_while(|(index, notification)| {
            let gap = if index == 0 { 0 } else { STACK_GAP };
            if remaining < gap + 3 {
                return None;
            }
            remaining -= gap;
            let height = popup_height(notification.message(), width).min(remaining);
            remaining -= height;
            Some(height)
        })
        .collect()
}

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
        let area = popup_area(frame_area, self.notification.message());
        self.render_at(frame, area);
    }

    fn render_at(&self, frame: &mut Frame, area: Rect) {
        let styles = self.theme.ui();
        let surface = styles.popup_surface();
        let accent = match self.notification.kind() {
            NotificationKind::Info => styles.action_hint,
            NotificationKind::Warning => styles.warning,
            NotificationKind::Error => styles.error,
        };
        let content = Paragraph::new(self.notification.message().to_string())
            .wrap(Wrap { trim: false })
            .style(surface.patch(styles.base));
        let title = self
            .notification
            .title()
            .unwrap_or(match self.notification.kind() {
                NotificationKind::Info => "Info",
                NotificationKind::Warning => "Warning",
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
    let height = popup_height(message, width).min(frame.height);
    let right_margin = u16::from(frame.width > width);
    let top_margin = u16::from(frame.height > height);
    Rect::new(
        frame.x + frame.width.saturating_sub(width + right_margin),
        frame.y + top_margin,
        width,
        height,
    )
}

fn popup_height(message: &str, width: u16) -> u16 {
    let text_width = width.saturating_sub(2).max(1) as usize;
    let lines = message
        .lines()
        .map(|line| {
            let width = UnicodeWidthStr::width(line);
            width.max(1).div_ceil(text_width)
        })
        .sum::<usize>()
        .max(1);
    (lines as u16 + 2).clamp(3, MAX_HEIGHT)
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
    fn stack_places_newest_first_and_pushes_older_messages_down() {
        let theme = Theme::default();
        let newest = Notification::info("newest");
        let older = Notification::info("older");
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                NotificationStack::new([&newest, &older], &theme).render(frame, frame.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let row = |y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row(2).contains("newest"));
        assert!(row(6).contains("older"));
        assert_eq!(buffer[(31, 1)].symbol(), "┌");
        assert_eq!(buffer[(31, 5)].symbol(), "┌");
    }

    #[test]
    fn stack_omits_entries_that_do_not_fit_the_terminal() {
        let theme = Theme::default();
        let first = Notification::info("first");
        let second = Notification::info("second");
        let third = Notification::info("third");
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                NotificationStack::new([&first, &second, &third], &theme)
                    .render(frame, frame.area());
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("first"));
        assert!(content.contains("second"));
        assert!(!content.contains("third"));
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
