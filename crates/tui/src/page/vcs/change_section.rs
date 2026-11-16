use ratatui::{Frame, layout::Rect, style::Modifier, text::Line, widgets::Paragraph};

use ::vcs::{ChangeKind, FileChange};

use crate::{
    component::{diff, picker},
    data_object::Model,
    vcs::ChangesState,
};

pub(super) struct ChangeSection<'a> {
    model: &'a Model,
    staged: bool,
}

impl<'a> ChangeSection<'a> {
    pub(super) fn staged(model: &'a Model) -> Self {
        Self::new(model, true)
    }

    pub(super) fn unstaged(model: &'a Model) -> Self {
        Self::new(model, false)
    }

    fn new(model: &'a Model, staged: bool) -> Self {
        Self { model, staged }
    }

    /// Render one section and return the height consumed by the next layout.
    pub(super) fn render(&self, area: Rect, frame: &mut Frame) -> u16 {
        let (lines, selected) = self.lines();
        if lines.is_empty() || area.height == 0 {
            return 0;
        }
        let styles = self.model.theme.ui();
        let title = if self.staged {
            "Staged files"
        } else {
            "Unstaged files"
        };
        frame.render_widget(
            Paragraph::new(title).style(styles.base.patch(styles.key_hint)),
            Rect { height: 1, ..area },
        );
        let content = Rect {
            y: area.y.saturating_add(1),
            height: area.height.saturating_sub(1),
            ..area
        };
        let lines = picker::visible_lines(lines, selected, content.height);
        let rendered = lines.len() as u16;
        frame.render_widget(Paragraph::new(lines).style(styles.base), content);
        rendered.saturating_add(1)
    }

    /// Build section rows and the selected file's actual rendered row.
    fn lines(&self) -> (Vec<Line<'static>>, Option<usize>) {
        let ChangesState::Ready(changes) = self.model.vcs.changes() else {
            return (Vec::new(), None);
        };
        changes
            .iter()
            .enumerate()
            .filter(|(_, change)| change.staged == self.staged)
            .fold(
                (Vec::new(), None),
                |(mut lines, mut selected), (index, change)| {
                    if self.model.vcs.selected_change == Some(index) {
                        selected = Some(lines.len());
                    }
                    lines.push(change_line(self.model, index, change));
                    let expanded =
                        self.model
                            .vcs
                            .expanded_change
                            .as_ref()
                            .is_some_and(|(path, staged)| {
                                path == &change.path && *staged == change.staged
                            });
                    if expanded {
                        lines.extend(diff::lines(&self.model.vcs.diff, self.model.theme.ui()));
                    }
                    (lines, selected)
                },
            )
    }
}

fn change_line(model: &Model, index: usize, change: &FileChange) -> Line<'static> {
    let styles = model.theme.ui();
    let is_selected = model.vcs.selected_change == Some(index);
    let style = if is_selected {
        styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD)
    } else {
        styles.base
    };
    let marker = if is_selected { "> " } else { "  " };
    Line::styled(format!("{marker}{}", change_text(model, change)), style)
}

fn change_text(model: &Model, change: &FileChange) -> String {
    let marker = match change.kind {
        ChangeKind::Added => "A",
        ChangeKind::Untracked => "?",
        ChangeKind::Modified => "M",
        ChangeKind::Conflict => "C",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
    };
    let path = display_path(model, &change.path);
    match &change.previous_path {
        Some(previous) => format!("{marker} {} -> {path}", display_path(model, previous)),
        None => format!("{marker} {path}"),
    }
}

fn display_path(model: &Model, path: &std::path::Path) -> String {
    model
        .workspace
        .as_ref()
        .and_then(|workspace| path.strip_prefix(workspace.root()).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::DiffState;

    #[test]
    fn selection_tracks_the_row_after_an_expanded_diff() {
        let change = |path: &str| FileChange {
            kind: ChangeKind::Modified,
            staged: false,
            path: std::path::PathBuf::from(format!("/workspace/{path}")),
            previous_path: None,
        };
        let mut model = Model::default();
        model.vcs.changes = ChangesState::Ready(vec![change("first.rs"), change("second.rs")]);
        model.vcs.selected_change = Some(1);
        model.vcs.expanded_change = Some((std::path::PathBuf::from("/workspace/first.rs"), false));
        model.vcs.diff = DiffState::Ready {
            lines: vec!["+first".to_string()],
            truncated: false,
        };

        let (lines, selected) = ChangeSection::unstaged(&model).lines();

        assert_eq!(lines.len(), 3);
        assert_eq!(selected, Some(2));
    }
}
