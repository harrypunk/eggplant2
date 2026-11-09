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

impl HighlightKind {
    fn from_scope(scope: &str) -> Option<Self> {
        if scope.starts_with("comment") {
            Some(Self::Comment)
        } else if scope.starts_with("string") {
            Some(Self::String)
        } else if scope.starts_with("constant.numeric") || scope.starts_with("number") {
            Some(Self::Number)
        } else if scope.starts_with("boolean") {
            Some(Self::Boolean)
        } else if scope.starts_with("constant") {
            Some(Self::Constant)
        } else if scope.starts_with("function.macro") || scope.starts_with("macro") {
            Some(Self::Macro)
        } else if scope.starts_with("function") {
            Some(Self::Function)
        } else if scope.starts_with("type") {
            Some(Self::Type)
        } else if scope.starts_with("keyword") {
            Some(Self::Keyword)
        } else if scope.starts_with("variable") {
            Some(Self::Variable)
        } else if scope.starts_with("operator") {
            Some(Self::Operator)
        } else {
            None
        }
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

    loop {
        let next = match highlighter.next_event_offset() {
            u32::MAX => range_end,
            offset => (offset as usize).min(range_end).max(previous),
        };
        if let Some(kind) = active.last().copied() {
            append_highlight(&mut lines, first_line, text, previous, next, kind);
        }
        if next >= range_end {
            break;
        }

        let (event, highlights) = highlighter.advance();
        match event {
            HighlightEvent::Refresh => active = active_kinds(highlights, &scopes),
            HighlightEvent::Push => active.extend(active_kinds(highlights, &scopes)),
        }
        previous = next;
    }
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
    start: usize,
    end: usize,
    kind: HighlightKind,
) {
    for (index, line) in lines.iter_mut().enumerate() {
        let line_start = text.line_to_byte(first_line + index);
        let line_end = line_start + line.text.len();
        let span_start = start.max(line_start);
        let span_end = end.min(line_end);
        if span_start >= span_end {
            continue;
        }

        let start_byte = span_start - line_start;
        let end_byte = span_end - line_start;
        if let Some(last) = line.highlights.last_mut()
            && last.kind == kind
            && last.end_byte == start_byte
        {
            last.end_byte = end_byte;
            continue;
        }
        line.highlights.push(HighlightSpan {
            start_byte,
            end_byte,
            kind,
        });
    }
}
