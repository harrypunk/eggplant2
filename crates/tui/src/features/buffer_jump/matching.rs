use editor::{DocumentPosition, TextViewSnapshot};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Minimal visible-text interface required for matching and display geometry.
pub(crate) trait VisibleJumpView {
    fn line_count(&self) -> usize;
    fn line_text(&self, row: usize) -> Option<&str>;
    fn first_line(&self) -> usize;
    fn horizontal_offset(&self) -> usize;
}

impl VisibleJumpView for TextViewSnapshot {
    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn line_text(&self, row: usize) -> Option<&str> {
        self.lines.get(row).map(|line| line.text.as_str())
    }

    fn first_line(&self) -> usize {
        self.first_line
    }

    fn horizontal_offset(&self) -> usize {
        self.horizontal_offset
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JumpMatch {
    pub position: DocumentPosition,
    pub cell_width: usize,
}

/// Find non-overlapping smart-case matches in the visible text only.
pub(crate) fn find_visible_matches(
    view: &impl VisibleJumpView,
    viewport_width: usize,
    pattern: &[char],
) -> Vec<JumpMatch> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let case_sensitive = pattern.iter().any(|character| character.is_uppercase());
    let mut matches = Vec::new();
    for row in 0..view.line_count() {
        let Some(line) = view.line_text(row) else {
            continue;
        };
        let characters = line.chars().collect::<Vec<_>>();
        let mut index = 0;
        while index + pattern.len() <= characters.len() {
            if characters[index..index + pattern.len()]
                .iter()
                .zip(pattern)
                .all(|(actual, expected)| {
                    *actual == *expected
                        || (!case_sensitive && actual.to_lowercase().eq(expected.to_lowercase()))
                })
            {
                let position = DocumentPosition {
                    line: view.first_line() + row,
                    character: index,
                };
                let cell_width = characters[index..index + pattern.len()]
                    .iter()
                    .map(|character| UnicodeWidthChar::width(*character).unwrap_or(0))
                    .sum();
                if visible_cell(view, viewport_width, position, cell_width).is_some() {
                    matches.push(JumpMatch {
                        position,
                        cell_width,
                    });
                }
                index += pattern.len();
            } else {
                index += 1;
            }
        }
    }
    matches
}

pub(crate) fn following_character(
    view: &impl VisibleJumpView,
    target: &JumpMatch,
) -> Option<JumpMatch> {
    let row = target.position.line.checked_sub(view.first_line())?;
    let line = view.line_text(row)?;
    let character = target.position.character + 1;
    let next = line.chars().nth(character)?;
    let cell_width = UnicodeWidthChar::width(next).unwrap_or(0);
    Some(JumpMatch {
        position: DocumentPosition {
            line: target.position.line,
            character,
        },
        cell_width,
    })
}

pub(crate) fn visible_cell(
    text: &impl VisibleJumpView,
    viewport_width: usize,
    position: DocumentPosition,
    cell_width: usize,
) -> Option<(usize, usize)> {
    let row = position.line.checked_sub(text.first_line())?;
    let line = text.line_text(row)?;
    let byte = line
        .char_indices()
        .nth(position.character)
        .map_or(line.len(), |(byte, _)| byte);
    let display_column = UnicodeWidthStr::width(&line[..byte]);
    let column = display_column.checked_sub(text.horizontal_offset())?;
    (column + cell_width <= viewport_width).then_some((row, column))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestView {
        lines: Vec<String>,
        first_line: usize,
        horizontal_offset: usize,
    }

    impl VisibleJumpView for TestView {
        fn line_count(&self) -> usize {
            self.lines.len()
        }

        fn line_text(&self, row: usize) -> Option<&str> {
            self.lines.get(row).map(String::as_str)
        }

        fn first_line(&self) -> usize {
            self.first_line
        }

        fn horizontal_offset(&self) -> usize {
            self.horizontal_offset
        }
    }

    #[test]
    fn one_character_search_streams_visible_matches_through_the_view_interface() {
        let view = TestView {
            lines: vec!["hello".to_string(), "🙂else".to_string()],
            first_line: 8,
            horizontal_offset: 0,
        };
        let matches = find_visible_matches(&view, 20, &['e']);
        assert_eq!(
            matches,
            vec![
                JumpMatch {
                    position: DocumentPosition {
                        line: 8,
                        character: 1,
                    },
                    cell_width: 1,
                },
                JumpMatch {
                    position: DocumentPosition {
                        line: 9,
                        character: 1,
                    },
                    cell_width: 1,
                },
                JumpMatch {
                    position: DocumentPosition {
                        line: 9,
                        character: 4,
                    },
                    cell_width: 1,
                },
            ]
        );
    }

    #[test]
    fn following_character_hints_include_wide_cells_and_skip_line_endings() {
        let view = TestView {
            lines: vec!["hello".to_string(), "e🙂".to_string(), "else".to_string()],
            first_line: 0,
            horizontal_offset: 0,
        };
        let matches = find_visible_matches(&view, 20, &['e']);
        let hints = matches
            .iter()
            .filter_map(|target| following_character(&view, target))
            .collect::<Vec<_>>();

        assert_eq!(
            hints,
            vec![
                JumpMatch {
                    position: DocumentPosition {
                        line: 0,
                        character: 2,
                    },
                    cell_width: 1,
                },
                JumpMatch {
                    position: DocumentPosition {
                        line: 1,
                        character: 1,
                    },
                    cell_width: 2,
                },
                JumpMatch {
                    position: DocumentPosition {
                        line: 2,
                        character: 1,
                    },
                    cell_width: 1,
                },
            ]
        );
    }
}
