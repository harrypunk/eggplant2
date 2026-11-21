use ratatui::layout::{Rect, Size};

use super::{text_input::TextInput, text_layout::TextFlow};

/// Reusable auto-height policy for dialogs with a measured text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoHeight {
    min_height: u16,
    max_height: u16,
    chrome_width: u16,
    chrome_height: u16,
    reserved_height: u16,
}

impl AutoHeight {
    pub const fn new(
        min_height: u16,
        max_height: u16,
        chrome_width: u16,
        chrome_height: u16,
        reserved_height: u16,
    ) -> Self {
        Self {
            min_height,
            max_height,
            chrome_width,
            chrome_height,
            reserved_height,
        }
    }

    pub fn dialog_size(&self, input: &TextInput, width: u16, flow: TextFlow) -> Size {
        let height = input
            .measure(width, flow)
            .height
            .saturating_add(self.chrome_height)
            .clamp(self.min_height, self.max_height);
        Size::new(width.saturating_add(self.chrome_width), height)
    }

    pub fn input_height(&self, input: &TextInput, area: Rect, flow: TextFlow) -> u16 {
        input
            .measure(area.width, flow)
            .height
            .min(area.height.saturating_sub(self.reserved_height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_clamps_dialog_and_input_independently() {
        let policy = AutoHeight::new(8, 12, 2, 4, 2);
        let input = TextInput::new("x".repeat(80));
        assert_eq!(
            policy.dialog_size(&input, 20, TextFlow::Wrapped),
            Size::new(22, 11)
        );
        assert_eq!(
            policy.input_height(&input, Rect::new(0, 0, 20, 6), TextFlow::Wrapped),
            4
        );
    }
}
