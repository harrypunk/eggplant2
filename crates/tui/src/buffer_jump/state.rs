use editor::{BufferId, DocumentPosition, TextViewSnapshot};
use unicode_width::UnicodeWidthStr;

const LABEL_KEYS: &[u8] = b"zxcvbnmasdfghjklqwertyuiop";
pub(crate) const LABEL_WIDTH: usize = 2;
const MAX_TARGETS: usize = LABEL_KEYS.len() * LABEL_KEYS.len();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JumpTarget {
    pub position: DocumentPosition,
    pub label: String,
}

/// State for one visible-buffer jump session.
#[derive(Debug, Default)]
pub struct BufferJumpState {
    buffer: Option<BufferId>,
    pattern: String,
    label: String,
    targets: Vec<JumpTarget>,
}

impl BufferJumpState {
    pub fn is_active(&self) -> bool {
        self.buffer.is_some()
    }

    pub(crate) fn buffer(&self) -> Option<BufferId> {
        self.buffer
    }

    pub(crate) fn pattern(&self) -> &str {
        &self.pattern
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn targets(&self) -> &[JumpTarget] {
        &self.targets
    }

    pub(crate) fn is_label_phase(&self) -> bool {
        self.pattern.chars().count() == LABEL_WIDTH
    }

    pub(crate) fn start(&mut self, buffer: BufferId) {
        *self = Self {
            buffer: Some(buffer),
            ..Self::default()
        };
    }

    pub(crate) fn close(&mut self) {
        *self = Self::default();
    }

    /// Add a pattern character and report whether the two-character pattern is complete.
    pub(crate) fn push_pattern(&mut self, character: char) -> bool {
        self.pattern.push(character);
        self.is_label_phase()
    }

    /// Match the visible snapshot once and assign stable labels to the results.
    pub(crate) fn assign_targets(&mut self, text: &TextViewSnapshot, viewport_width: usize) {
        let pattern = self.pattern.chars().collect::<Vec<_>>();
        let Ok(pattern): Result<[char; LABEL_WIDTH], _> = pattern.try_into() else {
            self.targets.clear();
            return;
        };
        self.targets = visible_targets(text, viewport_width, pattern);
    }

    pub(crate) fn push_label(&mut self, character: char) {
        self.label.push(character);
    }

    pub(crate) fn complete_label(&self) -> Option<Option<DocumentPosition>> {
        if self.label.chars().count() < LABEL_WIDTH {
            return None;
        }
        Some(
            self.targets
                .iter()
                .find(|target| target.label == self.label)
                .map(|target| target.position),
        )
    }

    pub(crate) fn clear_label(&mut self) {
        self.label.clear();
    }

    pub(crate) fn backspace(&mut self) {
        if self.label.pop().is_some() {
            return;
        }
        if self.is_label_phase() {
            self.targets.clear();
        }
        self.pattern.pop();
    }
}

fn visible_targets(
    text: &TextViewSnapshot,
    viewport_width: usize,
    pattern: [char; LABEL_WIDTH],
) -> Vec<JumpTarget> {
    let pattern = pattern.into_iter().collect::<String>();
    text.lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            line.text
                .match_indices(&pattern)
                .map(|(byte, _)| DocumentPosition {
                    line: text.first_line + row,
                    character: line.text[..byte].chars().count(),
                })
                .collect::<Vec<_>>()
        })
        .filter(|position| visible_cell(text, viewport_width, *position).is_some())
        .take(MAX_TARGETS)
        .enumerate()
        .map(|(index, position)| JumpTarget {
            position,
            label: label_for(index),
        })
        .collect()
}

pub(crate) fn visible_cell(
    text: &TextViewSnapshot,
    viewport_width: usize,
    position: DocumentPosition,
) -> Option<(usize, usize)> {
    let row = position.line.checked_sub(text.first_line)?;
    let line = text.lines.get(row)?;
    let byte = line
        .text
        .char_indices()
        .nth(position.character)
        .map_or(line.text.len(), |(byte, _)| byte);
    let display_column = UnicodeWidthStr::width(&line.text[..byte]);
    let column = display_column.checked_sub(text.horizontal_offset)?;
    (column + LABEL_WIDTH <= viewport_width).then_some((row, column))
}

fn label_for(index: usize) -> String {
    let first = LABEL_KEYS[index / LABEL_KEYS.len()];
    let second = LABEL_KEYS[(index + 1) % LABEL_KEYS.len()];
    String::from_utf8(vec![first, second]).expect("label alphabet must remain ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_snapshot_lines_and_assigns_stable_labels() {
        let mut text = TextViewSnapshot::plain(vec![
            "hello".to_string(),
            "goodbye".to_string(),
            "else".to_string(),
        ]);
        text.first_line = 10;
        text.horizontal_offset = 0;

        let targets = visible_targets(&text, 20, ['e', 'l']);

        assert_eq!(
            targets,
            vec![
                JumpTarget {
                    position: DocumentPosition {
                        line: 10,
                        character: 1,
                    },
                    label: "zx".to_string(),
                },
                JumpTarget {
                    position: DocumentPosition {
                        line: 12,
                        character: 0,
                    },
                    label: "zc".to_string(),
                },
            ]
        );
    }

    #[test]
    fn unicode_columns_and_horizontal_clipping_use_display_cells() {
        let mut text = TextViewSnapshot::plain(vec!["🙂hello".to_string()]);
        let target = DocumentPosition {
            line: 0,
            character: 2,
        };
        assert_eq!(visible_cell(&text, 6, target), Some((0, 3)));
        assert_eq!(visible_cell(&text, 4, target), None);

        text.horizontal_offset = 3;
        assert_eq!(visible_cell(&text, 3, target), Some((0, 0)));
        text.horizontal_offset = 5;
        assert_eq!(visible_cell(&text, 20, target), None);
    }

    #[test]
    fn backspace_can_edit_either_pattern_or_label() {
        let mut state = BufferJumpState::default();
        state.push_pattern('e');
        state.push_pattern('l');
        state.push_label('z');
        state.backspace();
        assert_eq!(state.label(), "");
        assert_eq!(state.pattern(), "el");

        state.backspace();
        assert_eq!(state.pattern(), "e");
        assert!(!state.is_label_phase());
    }
}
