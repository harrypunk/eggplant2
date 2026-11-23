use ratatui::{
    Frame,
    layout::{Rect, Size},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    component::popup::Popup,
    data_object::Model,
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

const HELP_TEXT: [&str; 10] = [
    "Normal",
    "  i              insert text",
    "  ? / Esc        close help",
    "  Ctrl-H / Ctrl-L move focus",
    "",
    "Insert",
    "  Enter          submit prompt",
    "  Esc            return to Normal",
    "  Backspace      delete previous character",
    "  Ctrl-F7        cancel active run",
];

pub(super) fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.agent.help_visible().then(|| {
        OverlayContribution::new(
            "agent_help",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(54, 12)),
            render,
        )
    })
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    let styles = model.theme.ui();
    let lines = HELP_TEXT.into_iter().map(Line::raw).collect::<Vec<_>>();
    let content = Paragraph::new(lines).style(styles.popup_surface());
    frame.render_widget(Popup::new("Agent Help", content, styles), area);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn help_is_a_popup_and_lists_both_input_modes() {
        let mut model = Model::default();
        assert!(overlay(&model).is_none());
        model.agent.toggle_help();

        let contribution = overlay(&model).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(54, 12)).unwrap();
        terminal
            .draw(|frame| {
                let area = contribution.area(frame.area(), frame.area());
                (contribution.render)(&model, frame, area);
            })
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for expected in [
            "Agent Help",
            "Normal",
            "insert text",
            "close help",
            "Insert",
            "submit prompt",
            "return to Normal",
        ] {
            assert!(content.contains(expected), "missing help text: {expected}");
        }
    }
}
