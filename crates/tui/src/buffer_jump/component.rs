use editor::EditorSnapshot;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};

use crate::{
    component::popup::Popup,
    data_object::Model,
    ui::{EditorOverlayContribution, OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::state::visible_cell;

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.buffer_jump.is_active().then(|| {
        OverlayContribution::new(
            "buffer_jump",
            OverlayKind::Modal,
            OverlayPlacement::BottomRight(Size::new(24, 3)),
            render_prompt,
        )
    })
}

pub(crate) fn editor_overlay(model: &Model) -> Option<EditorOverlayContribution> {
    (model.buffer_jump.is_active() && model.buffer_jump.buffer() == model.editor.active_buffer())
        .then(|| EditorOverlayContribution::new("buffer_jump", render_labels))
}

fn render_prompt(model: &Model, frame: &mut Frame, area: Rect) {
    let state = &model.buffer_jump;
    let phase = match state.is_label_phase() {
        true => "label",
        false => "two characters",
    };
    let input = match state.is_label_phase() {
        true => format!("{}  {}", state.pattern(), state.label()),
        false => state.pattern().to_string(),
    };
    let prompt = Paragraph::new(format!("> {input}"));
    frame.render_widget(
        Popup::new(format!("Jump: {phase}"), prompt, model.theme.ui()),
        area,
    );
}

fn render_labels(model: &Model, snapshot: &EditorSnapshot, area: Rect, buffer: &mut Buffer) {
    let state = &model.buffer_jump;
    if !state.is_label_phase() {
        return;
    }

    let dim_color = model.theme.ui().muted.fg.unwrap_or(Color::DarkGray);
    buffer.set_style(area, Style::new().fg(dim_color).add_modifier(Modifier::DIM));
    let label_style = model
        .theme
        .ui()
        .base
        .patch(model.theme.ui().selection)
        .patch(model.theme.ui().key_hint)
        .remove_modifier(Modifier::DIM);
    state.targets().iter().for_each(|target| {
        let Some((row, column)) = visible_cell(
            &snapshot.text,
            area.width.into(),
            target.position,
            target.label.len(),
        ) else {
            return;
        };
        let Ok(column) = u16::try_from(column) else {
            return;
        };
        let Ok(row) = u16::try_from(row) else {
            return;
        };
        if column >= area.width || row >= area.height {
            return;
        }
        buffer.set_stringn(
            area.x + column,
            area.y + row,
            &target.label,
            target.label.len(),
            label_style,
        );
    });
}
