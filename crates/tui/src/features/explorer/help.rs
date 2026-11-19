use ratatui::{Frame, layout::Size, text::Line, widgets::Paragraph};

use crate::{
    component::popup::Popup,
    data_object::Model,
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

const HELP_TEXT: [&str; 14] = [
    "j / k       next / previous entry",
    "gg / G       first / last entry",
    "Enter       open file / unfold folder",
    "h / l       fold / unfold folder",
    "r           refresh tree",
    "Shift-I     show / hide ignored entries",
    "Ctrl-H/J/K/L move focus",
    "",
    "a           add file / directory",
    "d           delete (placeholder)",
    "m           move (placeholder)",
    "c           copy (placeholder)",
    "",
    "? / Esc     close help",
];

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.explorer.help_visible().then(|| {
        OverlayContribution::new(
            "explorer_help",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(52, 16)),
            render,
        )
    })
}

fn render(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
    let styles = model.theme.ui();
    let lines = HELP_TEXT.into_iter().map(Line::raw).collect::<Vec<_>>();
    let content = Paragraph::new(lines).style(styles.popup_surface());
    frame.render_widget(Popup::new("Explorer Help", content, styles), area);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn help_lists_default_keys_and_marks_file_operations_as_placeholders() {
        let mut model = Model::default();
        assert!(overlay(&model).is_none());
        model.explorer.open_help();

        let contribution = overlay(&model).unwrap();
        let backend = TestBackend::new(60, 17);
        let mut terminal = Terminal::new(backend).unwrap();
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
            "Explorer Help",
            "next / previous entry",
            "first / last entry",
            "show / hide ignored entries",
            "add file / directory",
            "delete (placeholder)",
            "move (placeholder)",
            "copy (placeholder)",
            "? / Esc     close help",
        ] {
            assert!(content.contains(expected), "missing help text: {expected}");
        }
    }
}
