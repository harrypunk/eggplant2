use std::collections::BTreeSet;

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
            ModelSelectorPage::Loading => Paragraph::new(operation_lines(
                "Loading available models",
                "Provider catalogs",
                styles,
            )),
            ModelSelectorPage::Switching => Paragraph::new(operation_lines(
                "Activating selected model",
                &state
                    .selected_model()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "selected model".to_string()),
                styles,
            )),
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
    let providers = state
        .models()
        .iter()
        .map(|model| model.provider_id())
        .collect::<BTreeSet<_>>()
        .len() as u16;
    let rows = u16::try_from(state.models().len())
        .unwrap_or(MAX_MODEL_ROWS)
        .saturating_add(providers)
        .min(MAX_MODEL_ROWS);
    rows.saturating_add(7)
        .saturating_add(u16::from(state.notice().is_some()))
}

fn model_lines(state: &ModelSelectorState, styles: &UiTheme, height: u16) -> Vec<Line<'static>> {
    let notice_height = u16::from(state.notice().is_some());
    let viewport = height.saturating_sub(5 + notice_height);
    let mut provider = None;
    let mut selected_row = None;
    let mut options = Vec::new();
    for (index, model) in state.models().iter().enumerate() {
        if provider != Some(model.provider_id()) {
            options.push(Line::styled(
                model.provider_id().to_string(),
                styles.action_hint,
            ));
            provider = Some(model.provider_id());
        }
        let selected = state.selected() == Some(index);
        if selected {
            selected_row = Some(options.len());
        }
        options.push(picker::selectable_line(
            format!("  {}", model.model_id()),
            selected,
            styles,
        ));
    }
    let options = picker::visible_lines(options, selected_row, viewport);
    let mut lines = vec![
        Line::styled("Select a model", styles.action_hint),
        Line::raw(""),
    ];
    lines.extend(options);
    let active = state
        .active_model()
        .map(ToString::to_string)
        .unwrap_or_else(|| "none selected".to_string());
    if state.page() == ModelSelectorPage::Refreshing {
        lines.push(Line::styled(
            format!("Active: {active} · refreshing..."),
            styles.muted,
        ));
    } else {
        lines.push(Line::styled(format!("Active: {active}"), styles.muted));
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

fn operation_lines(label: &'static str, detail: &str, styles: &UiTheme) -> Vec<Line<'static>> {
    vec![
        Line::styled(label, styles.action_hint),
        Line::raw(""),
        Line::styled(detail.to_string(), styles.base),
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
    use crate::{model_selector::ProviderModel, theme::Theme};

    fn catalog(provider_id: &str, models: Vec<String>) -> ModelCatalog {
        ModelCatalog::new(provider_id, models, 1).unwrap()
    }

    fn render(state: &ModelSelectorState, theme: &Theme) -> String {
        let size = ModelSelector::size(state);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal
            .draw(|frame| ModelSelector::render(state, theme.ui(), frame, frame.area()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn catalog_rows_stay_visible_and_show_refresh_status() {
        let theme = Theme::default();
        let mut state = ModelSelectorState::default();
        state.open(None, Some(ProviderModel::new("openai", "model-19")));
        state.apply_cached(vec![catalog(
            "openai",
            (0..20).map(|index| format!("model-{index:02}")).collect(),
        )]);
        let content = render(&state, &theme);

        assert!(content.contains("model-19"));
        assert!(!content.contains("model-00"));
        assert!(content.contains("Active: openai/model-19"));
    }

    #[test]
    fn providers_render_as_sections_with_indented_models() {
        let theme = Theme::default();
        let mut state = ModelSelectorState::default();
        state.open(None, None);
        state.apply_cached(vec![
            catalog("openai", vec!["gpt-a".to_string()]),
            catalog(
                "qwen-tokenplan-individual-cn",
                vec!["qwen-a".to_string(), "qwen-b".to_string()],
            ),
        ]);
        let content = render(&state, &theme);

        let openai = content.find("openai").unwrap();
        let gpt = content.find("gpt-a").unwrap();
        let qwen = content.find("qwen-tokenplan-individual-cn").unwrap();
        let qwen_a = content.find("qwen-a").unwrap();
        let qwen_b = content.find("qwen-b").unwrap();
        assert!(openai < gpt && gpt < qwen && qwen < qwen_a && qwen_a < qwen_b);
        assert!(content.contains(">   gpt-a"));
    }
}
