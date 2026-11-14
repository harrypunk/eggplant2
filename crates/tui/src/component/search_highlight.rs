use std::ops::Range;

use editor::TextViewSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

use crate::{component::text_view::TextView, theme::UiTheme};

pub fn match_highlight() -> Style {
    Style::new()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn highlighted_spans(
    text: &str,
    ranges: &[Range<usize>],
    base: Style,
    highlight: Style,
) -> Vec<Span<'static>> {
    let ranges = ranges.iter().filter(|range| valid_range(text, range)).fold(
        Vec::<Range<usize>>::new(),
        |mut ranges, range| {
            if ranges.last().is_none_or(|last| last.end <= range.start) {
                ranges.push(range.clone());
            }
            ranges
        },
    );
    let trailing_offset = ranges.last().map_or(0, |range| range.end);
    let spans = std::iter::once(0)
        .chain(ranges.iter().map(|range| range.end))
        .zip(&ranges)
        .flat_map(|(offset, range)| {
            [
                (offset < range.start)
                    .then(|| Span::styled(text[offset..range.start].to_string(), base)),
                (range.start < range.end)
                    .then(|| Span::styled(text[range.start..range.end].to_string(), highlight)),
            ]
            .into_iter()
            .flatten()
        })
        .chain(
            (trailing_offset < text.len())
                .then(|| Span::styled(text[trailing_offset..].to_string(), base)),
        )
        .collect::<Vec<_>>();
    match spans.is_empty() {
        true => vec![Span::styled(text.to_string(), base)],
        false => spans,
    }
}

pub fn style_preview<'a>(
    snapshot: &TextViewSnapshot,
    selected_line: Option<usize>,
    matches: impl IntoIterator<Item = (usize, &'a [Range<usize>])>,
    area: Rect,
    buffer: &mut Buffer,
    styles: &UiTheme,
) {
    let selected = selected_line
        .and_then(|line| line.checked_sub(snapshot.first_line))
        .filter(|row| *row < snapshot.lines.len())
        .and_then(|row| u16::try_from(row).ok())
        .filter(|row| *row < area.height)
        .map(|row| {
            (
                Rect::new(area.x, area.y + row, area.width, 1),
                styles.selection,
            )
        });
    let text_area = TextView::text_area(area, snapshot);
    let matches = matches
        .into_iter()
        .flat_map(|(line, ranges)| result_match_areas(line, ranges, snapshot, text_area));

    selected
        .into_iter()
        .chain(matches.map(|area| (area, match_highlight())))
        .for_each(|(area, style)| buffer.set_style(area, style));
}

fn result_match_areas<'a>(
    document_line: usize,
    ranges: &'a [Range<usize>],
    snapshot: &'a TextViewSnapshot,
    text_area: Rect,
) -> impl Iterator<Item = Rect> + 'a {
    let line = document_line
        .checked_sub(snapshot.first_line)
        .and_then(|row| snapshot.lines.get(row).map(|line| (row, line)));
    ranges.iter().filter_map(move |range| {
        let (row, line) = line?;
        preview_match_area(text_area, row, &line.text, range)
    })
}

fn preview_match_area(
    text_area: Rect,
    row: usize,
    text: &str,
    range: &Range<usize>,
) -> Option<Rect> {
    let row = u16::try_from(row)
        .ok()
        .filter(|row| *row < text_area.height)?;
    if !valid_range(text, range) {
        return None;
    }
    let start = UnicodeWidthStr::width(&text[..range.start]);
    let end = UnicodeWidthStr::width(&text[..range.end]);
    let viewport_width = usize::from(text_area.width);
    if start >= viewport_width || end <= start {
        return None;
    }
    Some(Rect::new(
        text_area.x + u16::try_from(start).ok()?,
        text_area.y + row,
        u16::try_from(end.min(viewport_width) - start).ok()?,
        1,
    ))
}

fn valid_range(text: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && range.end <= text.len()
        && text.is_char_boundary(range.start)
        && text.is_char_boundary(range.end)
}
