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

/// Theme-independent semantic style for a UTF-8 byte range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Keyword,
    String,
    Comment,
    Function,
    Type,
    Constant,
    Number,
    Boolean,
    Variable,
    Operator,
    Macro,
}

const SCOPE_KINDS: &[(&str, HighlightKind)] = &[
    ("comment", HighlightKind::Comment),
    ("string", HighlightKind::String),
    ("constant.numeric", HighlightKind::Number),
    ("number", HighlightKind::Number),
    ("boolean", HighlightKind::Boolean),
    ("constant", HighlightKind::Constant),
    ("function.macro", HighlightKind::Macro),
    ("macro", HighlightKind::Macro),
    ("function", HighlightKind::Function),
    ("type", HighlightKind::Type),
    ("keyword", HighlightKind::Keyword),
    ("variable", HighlightKind::Variable),
    ("operator", HighlightKind::Operator),
];

impl HighlightKind {
    fn from_scope(scope: &str) -> Option<Self> {
        // Ordered from specific to general prefixes.
        SCOPE_KINDS
            .iter()
            .find_map(|(prefix, kind)| scope.starts_with(prefix).then_some(*kind))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub kind: HighlightKind,
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
            Some(last) if last.kind == span.kind && last.end_byte == span.start_byte => {
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
    let scopes = loader.scopes();
    let mut highlighter = syntax.highlighter(
        source,
        &loader,
        u32::try_from(range_start).unwrap_or(u32::MAX)
            ..u32::try_from(range_end).unwrap_or(u32::MAX),
    );
    let mut active = active_kinds(highlighter.active_highlights(), &scopes);
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
        let kind = active.last().copied();

        if next < range_end {
            let (event, highlights) = highlighter.advance();
            match event {
                HighlightEvent::Refresh => active = active_kinds(highlights, &scopes),
                HighlightEvent::Push => active.extend(active_kinds(highlights, &scopes)),
            }
        }
        previous = next;
        Some((interval, kind))
    });

    intervals
        .filter_map(|(range, kind)| kind.map(|kind| (range, kind)))
        .filter(|(range, _)| !range.is_empty())
        .for_each(|(range, kind)| {
            append_highlight(&mut lines, first_line, text, range, kind);
        });
    lines
}

fn active_kinds(
    highlights: impl Iterator<Item = Highlight>,
    scopes: &[String],
) -> Vec<HighlightKind> {
    highlights
        .filter_map(|highlight| {
            scopes
                .get(highlight.idx())
                .and_then(|scope| HighlightKind::from_scope(scope))
        })
        .collect()
}

fn append_highlight(
    lines: &mut [LineSnapshot],
    first_line: usize,
    text: &helix_core::Rope,
    range: Range<usize>,
    kind: HighlightKind,
) {
    lines.iter_mut().enumerate().for_each(|(index, line)| {
        let line_start = text.line_to_byte(first_line + index);
        let line_end = line_start + line.text.len();
        clip_highlight(range.clone(), line_start, line_end, kind)
            .into_iter()
            .for_each(|span| line.push_highlight(span));
    });
}

fn clip_highlight(
    range: Range<usize>,
    line_start: usize,
    line_end: usize,
    kind: HighlightKind,
) -> Option<HighlightSpan> {
    let start = range.start.max(line_start);
    let end = range.end.min(line_end);
    (start < end).then_some(HighlightSpan {
        start_byte: start - line_start,
        end_byte: end - line_start,
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_specific_scope_prefixes_before_general_ones() {
        assert_eq!(
            HighlightKind::from_scope("function.macro"),
            Some(HighlightKind::Macro)
        );
        assert_eq!(
            HighlightKind::from_scope("constant.numeric"),
            Some(HighlightKind::Number)
        );
        assert_eq!(
            HighlightKind::from_scope("keyword.control"),
            Some(HighlightKind::Keyword)
        );
        assert_eq!(HighlightKind::from_scope("punctuation"), None);
    }
}
