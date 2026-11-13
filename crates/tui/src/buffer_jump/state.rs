use editor::{BufferId, DocumentPosition, TextViewSnapshot};
use unicode_width::UnicodeWidthStr;

const PATTERN_LENGTH: usize = 2;
const LABEL_KEYS: &[u8] = b"zxcvbnmasdfghjklqwertyuiop";
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
        self.pattern.chars().count() == PATTERN_LENGTH
    }

    fn label_length(&self) -> usize {
        self.targets
            .first()
            .map_or(1, |target| target.label.chars().count())
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
        let Ok(pattern): Result<[char; PATTERN_LENGTH], _> = pattern.try_into() else {
            self.targets.clear();
            return;
        };
        self.targets = visible_targets(text, viewport_width, pattern);
    }

    pub(crate) fn push_label(&mut self, character: char) {
        self.label.push(character);
    }

    pub(crate) fn complete_label(&self) -> Option<Option<DocumentPosition>> {
        if self.label.chars().count() < self.label_length() {
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
    pattern: [char; PATTERN_LENGTH],
) -> Vec<JumpTarget> {
    let case_sensitive = pattern.iter().any(|character| character.is_uppercase());
    let positions = text
        .lines
        .iter()
        .enumerate()
        .flat_map(|(row, line)| {
            let characters = line.text.chars().collect::<Vec<_>>();
            let mut matches = Vec::new();
            let mut index = 0;
            while index + PATTERN_LENGTH <= characters.len() {
                if characters[index..index + PATTERN_LENGTH]
                    .iter()
                    .zip(pattern)
                    .all(|(actual, expected)| {
                        *actual == expected
                            || (!case_sensitive
                                && actual.to_lowercase().eq(expected.to_lowercase()))
                    })
                {
                    matches.push(DocumentPosition {
                        line: text.first_line + row,
                        character: index,
                    });
                    index += PATTERN_LENGTH;
                } else {
                    index += 1;
                }
            }
            matches
        })
        .collect::<Vec<_>>();
    let label_length = if positions.len() <= LABEL_KEYS.len() {
        1
    } else {
        2
    };
    let max_targets = match label_length {
        1 => LABEL_KEYS.len(),
        _ => MAX_TARGETS,
    };

    positions
        .into_iter()
        .filter(|position| visible_cell(text, viewport_width, *position, label_length).is_some())
        .take(max_targets)
        .enumerate()
        .map(|(index, position)| JumpTarget {
            position,
            label: label_for(index, label_length),
        })
        .collect()
}

pub(crate) fn visible_cell(
    text: &TextViewSnapshot,
    viewport_width: usize,
    position: DocumentPosition,
    label_width: usize,
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
    (column + label_width <= viewport_width).then_some((row, column))
}

fn label_for(index: usize, length: usize) -> String {
    if length == 1 {
        return (LABEL_KEYS[index] as char).to_string();
    }
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
                    label: "z".to_string(),
                },
                JumpTarget {
                    position: DocumentPosition {
                        line: 12,
                        character: 0,
                    },
                    label: "x".to_string(),
                },
            ]
        );
    }

    #[test]
    fn matching_is_smart_case_sensitive() {
        let text = TextViewSnapshot::plain(
            ["hello", "HELLO", "Elite", "ELITE", "Élan"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        );

        assert_eq!(visible_targets(&text, 20, ['e', 'l']).len(), 4);
        assert_eq!(visible_targets(&text, 20, ['é', 'l']).len(), 1);
        let matches = visible_targets(&text, 20, ['E', 'l']);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].position.line, 2);
        let matches = visible_targets(&text, 20, ['E', 'L']);
        assert_eq!(matches.len(), 2);
        assert_eq!(
            matches
                .iter()
                .map(|target| target.position.line)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn many_matches_use_two_character_labels() {
        let text = TextViewSnapshot::plain(vec!["el".repeat(27)]);
        let mut state = BufferJumpState::default();
        state.push_pattern('e');
        state.push_pattern('l');
        state.assign_targets(&text, 100);

        assert_eq!(state.targets().len(), 27);
        assert_eq!(state.targets()[0].label, "zx");
        assert!(state.targets().iter().all(|target| target.label.len() == 2));
        let unique = state
            .targets()
            .iter()
            .map(|target| target.label.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), state.targets().len());

        state.push_label('z');
        assert_eq!(state.complete_label(), None);
        state.push_label('x');
        assert_eq!(
            state.complete_label(),
            Some(Some(DocumentPosition {
                line: 0,
                character: 0,
            }))
        );
    }

    #[test]
    fn unicode_columns_and_horizontal_clipping_use_display_cells() {
        let mut text = TextViewSnapshot::plain(vec!["🙂hello".to_string()]);
        let target = DocumentPosition {
            line: 0,
            character: 2,
        };
        assert_eq!(visible_cell(&text, 6, target, 2), Some((0, 3)));
        assert_eq!(visible_cell(&text, 4, target, 2), None);

        text.horizontal_offset = 3;
        assert_eq!(visible_cell(&text, 3, target, 2), Some((0, 0)));
        text.horizontal_offset = 5;
        assert_eq!(visible_cell(&text, 20, target, 2), None);
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
