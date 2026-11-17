use editor::{DocumentPosition, EditorSnapshot, TextViewSnapshot};
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
    theme::UiTheme,
    ui::{EditorOverlayContribution, OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::{
    matching::{JumpMatch, following_character},
    state::{JumpTarget, visible_cell},
};

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
        .then(|| EditorOverlayContribution::new("buffer_jump", render_overlay))
}

fn render_prompt(model: &Model, frame: &mut Frame, area: Rect) {
    let state = &model.buffer_jump;
    let phase = if state.is_label_phase() {
        "label"
    } else if state.pattern().is_empty() {
        "two characters"
    } else {
        "one more"
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

struct JumpStyles {
    dim: Style,
    matched: Style,
    label: Style,
}

impl JumpStyles {
    fn new(theme: &UiTheme) -> Self {
        let dim_color = theme.muted.fg.unwrap_or(Color::DarkGray);
        let highlighted = theme.base.patch(theme.selection);
        Self {
            dim: Style::new().fg(dim_color).add_modifier(Modifier::DIM),
            matched: highlighted
                .patch(theme.action_hint)
                .remove_modifier(Modifier::DIM),
            label: highlighted
                .patch(theme.key_hint)
                .remove_modifier(Modifier::DIM),
        }
    }
}

fn render_overlay(model: &Model, snapshot: &EditorSnapshot, area: Rect, buffer: &mut Buffer) {
    let state = &model.buffer_jump;
    let styles = JumpStyles::new(model.theme.ui());
    buffer.set_style(area, styles.dim);

    if state.is_label_phase() {
        draw_labels(state.targets(), snapshot, area, buffer, styles.label);
    } else {
        highlight_following_characters(state.matches(), snapshot, area, buffer, styles.matched);
    }
}

fn highlight_following_characters(
    matches: &[JumpMatch],
    snapshot: &EditorSnapshot,
    area: Rect,
    buffer: &mut Buffer,
    style: Style,
) {
    for target in matches {
        let Some(hint) = following_character(&snapshot.text, target) else {
            continue;
        };
        if let Some(target) = target_rect(&snapshot.text, area, hint.position, hint.cell_width) {
            buffer.set_style(target, style);
        }
    }
}

fn draw_labels(
    targets: &[JumpTarget],
    snapshot: &EditorSnapshot,
    area: Rect,
    buffer: &mut Buffer,
    style: Style,
) {
    for target in targets {
        if let Some(cell) = target_rect(&snapshot.text, area, target.position, target.label.len()) {
            buffer.set_stringn(cell.x, cell.y, &target.label, cell.width.into(), style);
        }
    }
}

fn target_rect(
    text: &TextViewSnapshot,
    area: Rect,
    position: DocumentPosition,
    cell_width: usize,
) -> Option<Rect> {
    let (row, column) = visible_cell(text, area.width.into(), position, cell_width)?;
    let row = u16::try_from(row).ok()?;
    let column = u16::try_from(column).ok()?;
    let width = u16::try_from(cell_width).ok()?;
    if column >= area.width || row >= area.height || width == 0 {
        return None;
    }
    Some(Rect::new(
        area.x + column,
        area.y + row,
        width.min(area.width - column),
        1,
    ))
}
