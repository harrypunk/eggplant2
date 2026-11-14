use std::ops::Range;

use helix_core::syntax::{Highlight, HighlightEvent};
use helix_view::{Document, View};

use crate::{
    document::default_syntax_loader,
    session::{BufferId, EditMode},
};

/// Cursor coordinates relative to the editor's visible text area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyState {
    Clean,
    Dirty,
}

impl From<bool> for DirtyState {
    fn from(modified: bool) -> Self {
        match modified {
            true => Self::Dirty,
            false => Self::Clean,
        }
    }
}

/// Exact Helix syntax highlights for a UTF-8 byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub highlights: Vec<Highlight>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSnapshot {
    pub text: String,
    pub highlights: Vec<HighlightSpan>,
}

impl LineSnapshot {
    pub fn plain(text: String) -> Self {
        Self {
            text,
            highlights: Vec::new(),
        }
    }

    fn push_highlight(&mut self, span: HighlightSpan) {
        match self.highlights.last_mut() {
            Some(last)
                if last.highlights == span.highlights && last.end_byte == span.start_byte =>
            {
                last.end_byte = span.end_byte;
            }
            _ => self.highlights.push(span),
        }
    }
}

/// Read-only projection of a document viewport, shared by editors and previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextViewSnapshot {
    /// Zero-based index of the first visible line in the full document.
    pub first_line: usize,
    /// Total logical line count, used for line-number gutter sizing.
    pub line_count: usize,
    pub lines: Vec<LineSnapshot>,
    pub horizontal_offset: usize,
}

impl TextViewSnapshot {
    pub fn plain(lines: Vec<String>) -> Self {
        let line_count = lines.len();
        Self {
            first_line: 0,
            line_count,
            lines: lines.into_iter().map(LineSnapshot::plain).collect(),
            horizontal_offset: 0,
        }
    }
}

/// Owned text and revision for background work against one buffer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferTextSnapshot {
    pub buffer_id: BufferId,
    pub revision: i32,
    pub text: String,
}

/// Filename-only buffer summary used by tabs and other buffer lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferInfo {
    pub id: BufferId,
    pub name: String,
    pub modified: DirtyState,
}

/// Owned, read-only data needed to render one active editor view.
///
/// This keeps Helix document/view types inside the editor crate. The snapshot
/// currently contains logical lines; visual soft-wrap rendering is deferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSnapshot {
    pub buffer_id: BufferId,
    pub display_name: String,
    pub buffers: Vec<BufferInfo>,
    pub mode: EditMode,
    pub modified: DirtyState,
    pub text: TextViewSnapshot,
    pub cursor: Option<CursorPosition>,
}

/// Project a Helix document/view without exposing either type to the TUI.
pub(crate) fn text_view_snapshot(document: &Document, view: &View) -> TextViewSnapshot {
    let offset = document.view_offset(view.id);
    let first_line = document
        .text()
        .char_to_line(offset.anchor.min(document.text().len_chars()));
    let end_line = (first_line + view.inner_height()).min(document.text().len_lines());
    let lines = (first_line..end_line)
        .map(|line| {
            LineSnapshot::plain(
                document
                    .text()
                    .line(line)
                    .to_string()
                    .trim_end_matches(['\r', '\n'])
                    .to_owned(),
            )
        })
        .collect::<Vec<_>>();

    TextViewSnapshot {
        first_line,
        line_count: document.text().len_lines(),
        lines: highlighted_lines(document, first_line, end_line, lines),
        horizontal_offset: offset.horizontal_offset,
    }
}

fn highlighted_lines(
    document: &Document,
    first_line: usize,
    end_line: usize,
    mut lines: Vec<LineSnapshot>,
) -> Vec<LineSnapshot> {
    let Some(syntax) = document.syntax() else {
        return lines;
    };
    let text = document.text();
    let source = text.slice(..);
    let range_start = text.line_to_byte(first_line);
    let range_end = match end_line == text.len_lines() {
        true => text.len_bytes(),
        false => text.line_to_byte(end_line),
    };
    let loader = default_syntax_loader();
    let loader = loader.load();
    let mut highlighter = syntax.highlighter(
        source,
        &loader,
        u32::try_from(range_start).unwrap_or(u32::MAX)
            ..u32::try_from(range_end).unwrap_or(u32::MAX),
    );
    let mut active = highlighter.active_highlights().collect::<Vec<_>>();
    let mut previous = range_start;
    let intervals = std::iter::from_fn(|| {
        if previous >= range_end {
            return None;
        }
        let next = match highlighter.next_event_offset() {
            u32::MAX => range_end,
            offset => (offset as usize).min(range_end).max(previous),
        };
        let interval = previous..next;
        let highlights = active.clone();

        if next < range_end {
            let (event, highlights) = highlighter.advance();
            update_active_highlights(&mut active, event, highlights);
        }
        previous = next;
        Some((interval, highlights))
    });

    intervals
        .filter(|(range, highlights)| !range.is_empty() && !highlights.is_empty())
        .for_each(|(range, highlights)| {
            append_highlight(&mut lines, first_line, text, range, &highlights);
        });
    lines
}

fn update_active_highlights(
    active: &mut Vec<Highlight>,
    event: HighlightEvent,
    highlights: impl IntoIterator<Item = Highlight>,
) {
    match event {
        HighlightEvent::Refresh => *active = highlights.into_iter().collect(),
        HighlightEvent::Push => active.extend(highlights),
    }
}

fn append_highlight(
    lines: &mut [LineSnapshot],
    first_line: usize,
    text: &helix_core::Rope,
    range: Range<usize>,
    highlights: &[Highlight],
) {
    lines.iter_mut().enumerate().for_each(|(index, line)| {
        let line_start = text.line_to_byte(first_line + index);
        let line_end = line_start + line.text.len();
        clip_highlight(range.clone(), line_start, line_end, highlights)
            .into_iter()
            .for_each(|span| line.push_highlight(span));
    });
}

fn clip_highlight(
    range: Range<usize>,
    line_start: usize,
    line_end: usize,
    highlights: &[Highlight],
) -> Option<HighlightSpan> {
    let start = range.start.max(line_start);
    let end = range.end.min(line_end);
    (start < end).then(|| HighlightSpan {
        start_byte: start - line_start,
        end_byte: end - line_start,
        highlights: highlights.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_replaces_and_push_extends_exact_active_highlights() {
        let outer = Highlight::new(3);
        let middle = Highlight::new(17);
        let inner = Highlight::new(41);
        let mut active = vec![outer];

        update_active_highlights(&mut active, HighlightEvent::Push, [middle, inner]);
        assert_eq!(active, vec![outer, middle, inner]);

        update_active_highlights(&mut active, HighlightEvent::Refresh, [inner]);
        assert_eq!(active, vec![inner]);
    }

    #[test]
    fn retains_layers_and_merges_only_matching_highlight_vectors() {
        let outer = Highlight::new(3);
        let inner = Highlight::new(41);
        let mut line = LineSnapshot::plain("abcdef".to_owned());

        line.push_highlight(HighlightSpan {
            start_byte: 0,
            end_byte: 2,
            highlights: vec![outer],
        });
        line.push_highlight(HighlightSpan {
            start_byte: 2,
            end_byte: 4,
            highlights: vec![outer, inner],
        });
        line.push_highlight(HighlightSpan {
            start_byte: 4,
            end_byte: 6,
            highlights: vec![outer, inner],
        });

        assert_eq!(
            line.highlights,
            vec![
                HighlightSpan {
                    start_byte: 0,
                    end_byte: 2,
                    highlights: vec![outer],
                },
                HighlightSpan {
                    start_byte: 2,
                    end_byte: 6,
                    highlights: vec![outer, inner],
                },
            ]
        );
    }

    #[test]
    fn ignores_spans_outside_a_line_without_subtracting_offsets() {
        let highlights = [Highlight::new(7), Highlight::new(11)];
        assert_eq!(clip_highlight(0..0, 10, 20, &highlights), None);
        assert_eq!(clip_highlight(30..40, 10, 20, &highlights), None);
        assert_eq!(
            clip_highlight(5..15, 10, 20, &highlights),
            Some(HighlightSpan {
                start_byte: 0,
                end_byte: 5,
                highlights: highlights.to_vec(),
            })
        );
    }
}
