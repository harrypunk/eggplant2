use std::path::Path;

use ratatui::{Frame, layout::Rect, style::Modifier, text::Line, widgets::Paragraph};

use ::vcs::{ChangeKind, FileChange};

use crate::{
    component::{diff, picker},
    theme::{Theme, UiTheme},
    vcs::{ChangesState, DiffState},
};

#[derive(Debug, Clone, Copy)]
pub(super) struct ChangeSectionProps<'a> {
    pub changes: &'a ChangesState,
    pub selected: Option<usize>,
    pub expanded: Option<(&'a Path, bool)>,
    pub diff: &'a DiffState,
    pub workspace_root: Option<&'a Path>,
    pub theme: &'a Theme,
}

pub(super) struct ChangeSection<'a> {
    props: ChangeSectionProps<'a>,
    staged: bool,
}

impl<'a> ChangeSection<'a> {
    pub(super) fn staged(props: ChangeSectionProps<'a>) -> Self {
        Self::new(props, true)
    }

    pub(super) fn unstaged(props: ChangeSectionProps<'a>) -> Self {
        Self::new(props, false)
    }

    fn new(props: ChangeSectionProps<'a>, staged: bool) -> Self {
        Self { props, staged }
    }

    /// Render one section and return the height consumed by the next layout.
    pub(super) fn render(&self, area: Rect, frame: &mut Frame) -> u16 {
        let (lines, selected) = self.lines();
        if lines.is_empty() || area.height == 0 {
            return 0;
        }
        let styles = self.props.theme.ui();
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
        let ChangesState::Ready(changes) = self.props.changes else {
            return (Vec::new(), None);
        };
        let styles = self.props.theme.ui();
        changes
            .iter()
            .enumerate()
            .filter(|(_, change)| change.staged == self.staged)
            .fold(
                (Vec::new(), None),
                |(mut lines, mut selected), (index, change)| {
                    let is_selected = self.props.selected == Some(index);
                    if is_selected {
                        selected = Some(lines.len());
                    }
                    lines.push(change_line(
                        change,
                        is_selected,
                        self.props.workspace_root,
                        styles,
                    ));
                    let expanded = self.props.expanded.is_some_and(|(path, staged)| {
                        path == change.path.as_path() && staged == change.staged
                    });
                    if expanded {
                        lines.extend(diff::lines(self.props.diff, styles));
                    }
                    (lines, selected)
                },
            )
    }
}

fn change_line(
    change: &FileChange,
    is_selected: bool,
    workspace_root: Option<&Path>,
    styles: &UiTheme,
) -> Line<'static> {
    let style = if is_selected {
        styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD)
    } else {
        styles.base
    };
    let marker = if is_selected { "> " } else { "  " };
    Line::styled(
        format!("{marker}{}", change_text(change, workspace_root)),
        style,
    )
}

fn change_text(change: &FileChange, workspace_root: Option<&Path>) -> String {
    let marker = match change.kind {
        ChangeKind::Added => "A",
        ChangeKind::Untracked => "?",
        ChangeKind::Modified => "M",
        ChangeKind::Conflict => "C",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
    };
    let path = display_path(workspace_root, &change.path);
    match &change.previous_path {
        Some(previous) => format!(
            "{marker} {} -> {path}",
            display_path(workspace_root, previous)
        ),
        None => format!("{marker} {path}"),
    }
}

fn display_path(workspace_root: Option<&Path>, path: &Path) -> String {
    workspace_root
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn selection_tracks_the_row_after_an_expanded_diff() {
        let change = |path: &str| FileChange {
            kind: ChangeKind::Modified,
            staged: false,
            path: PathBuf::from(format!("/workspace/{path}")),
            previous_path: None,
        };
        let changes = ChangesState::Ready(vec![change("first.rs"), change("second.rs")]);
        let expanded = PathBuf::from("/workspace/first.rs");
        let diff = DiffState::Ready {
            lines: vec!["+first".to_string()],
            truncated: false,
        };
        let theme = Theme::default();

        let (lines, selected) = ChangeSection::unstaged(ChangeSectionProps {
            changes: &changes,
            selected: Some(1),
            expanded: Some((&expanded, false)),
            diff: &diff,
            workspace_root: Some(Path::new("/workspace")),
            theme: &theme,
        })
        .lines();

        assert_eq!(lines.len(), 3);
        assert_eq!(selected, Some(2));
    }
}
