use ratatui::{
    Frame,
    layout::{Rect, Size},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    component::{picker, popup::Popup},
    model_selector::{ModelSelectorNotice, ModelSelectorPage, ModelSelectorState},
    theme::UiTheme,
};

const MAX_MODEL_ROWS: u16 = 10;

pub struct ModelSelector;

impl ModelSelector {
    pub fn size(state: &ModelSelectorState) -> Size {
        let height = match state.page() {
            ModelSelectorPage::Loading => 8,
            ModelSelectorPage::Switching => 8,
            ModelSelectorPage::Closed => 0,
            _ => model_height(state),
        };
        Size::new(64, height)
    }

    pub fn render(state: &ModelSelectorState, styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let content = match state.page() {
            ModelSelectorPage::Loading => {
                Paragraph::new(operation_lines("Loading available models", styles))
            }
            ModelSelectorPage::Switching => {
                Paragraph::new(operation_lines("Activating selected model", styles))
            }
            ModelSelectorPage::Closed => Paragraph::new(""),
            _ => Paragraph::new(model_lines(state, styles, area.height)),
        };
        frame.render_widget(
            Popup::new("Model", content.style(styles.popup_surface()), styles),
            area,
        );
    }
}

fn model_height(state: &ModelSelectorState) -> u16 {
    let rows = u16::try_from(state.models().len())
        .unwrap_or(MAX_MODEL_ROWS)
        .min(MAX_MODEL_ROWS);
    rows.saturating_add(7)
        .saturating_add(u16::from(state.notice().is_some()))
}

fn model_lines(state: &ModelSelectorState, styles: &UiTheme, height: u16) -> Vec<Line<'static>> {
    let notice_height = u16::from(state.notice().is_some());
    let viewport = height.saturating_sub(5 + notice_height);
    let options = state
        .models()
        .iter()
        .enumerate()
        .map(|(index, model)| {
            picker::selectable_line(model.clone(), state.selected() == Some(index), styles)
        })
        .collect();
    let options = picker::visible_lines(options, state.selected(), viewport);
    let mut lines = vec![
        Line::styled("Select a model", styles.action_hint),
        Line::raw(""),
    ];
    lines.extend(options);
    if state.page() == ModelSelectorPage::Refreshing {
        lines.push(Line::styled(
            format!(
                "Active: {} · refreshing...",
                state.active_model().unwrap_or("none selected")
            ),
            styles.muted,
        ));
    } else {
        lines.push(Line::styled(
            format!(
                "Active: {}",
                state.active_model().unwrap_or("none selected")
            ),
            styles.muted,
        ));
    }
    if let Some(notice) = state.notice() {
        let (text, style) = match notice {
            ModelSelectorNotice::Warning(text) => (text, styles.warning),
            ModelSelectorNotice::Error(text) => (text, styles.error),
        };
        lines.push(Line::styled(text.clone(), style));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Enter select · Esc close", styles.muted));
    lines
}

fn operation_lines(label: &'static str, styles: &UiTheme) -> Vec<Line<'static>> {
    vec![
        Line::styled(label, styles.action_hint),
        Line::raw(""),
        Line::styled("Provider: openai", styles.base),
        Line::styled("Please wait", styles.muted),
        Line::raw(""),
        Line::styled("Esc close", styles.muted),
    ]
}

#[cfg(test)]
mod tests {
    use eggplant_agent_storage::ModelCatalog;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn catalog_rows_stay_visible_and_show_refresh_status() {
        let theme = Theme::default();
        let mut state = ModelSelectorState::default();
        state.open(None, "openai".to_string(), Some("model-19".to_string()));
        state.apply_cached(
            ModelCatalog::new(
                "openai",
                (0..20).map(|index| format!("model-{index:02}")).collect(),
                1,
            )
            .unwrap(),
        );
        let size = ModelSelector::size(&state);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal
            .draw(|frame| ModelSelector::render(&state, theme.ui(), frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(content.contains("model-19"));
        assert!(!content.contains("model-00"));
        assert!(content.contains("Active: model-19"));
    }
}
