use std::path::Path;

use editor::{EditMode, EditorSnapshot};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Modifier,
    text::Line,
    widgets::Paragraph,
};

use ::vcs::{ChangeKind, FileChange};

use crate::{
    component::{
        editor::{Editor as EditorComponent, EditorBuffer},
        status_bar::StatusBar,
    },
    data_object::{FocusState, Model},
    theme::Theme,
    vcs::ChangesState,
};

#[derive(Debug, Default)]
pub struct VcsCommit;

struct CommitHeaderProps<'a> {
    changes: &'a ChangesState,
    workspace_root: Option<&'a Path>,
    notice: Option<&'a str>,
    theme: &'a Theme,
}

struct PageLayout {
    header: Rect,
    editor: Rect,
    status: Rect,
}

impl VcsCommit {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let areas = layout_areas(frame.area());
        let styles = model.theme.ui();
        let header = CommitHeaderProps {
            changes: model.vcs.changes(),
            workspace_root: model.workspace.as_ref().map(|workspace| workspace.root()),
            notice: model.vcs.notice.as_deref(),
            theme: &model.theme,
        };
        frame.render_widget(
            Paragraph::new(header_lines(header)).style(styles.base),
            areas.header,
        );
        let snapshot = model.vcs.commit.editor.active_snapshot();
        let editor = EditorComponent::new(snapshot.as_ref(), FocusState::Focused, &model.theme);
        let cursor = editor.cursor_position(areas.editor);
        frame.render_widget(editor, areas.editor);
        let mode = snapshot
            .as_ref()
            .map_or(EditMode::Normal, |snapshot| snapshot.mode);
        frame.render_widget(
            StatusBar::new(
                mode,
                None,
                "Commit message",
                "",
                model.status.as_ref(),
                None,
                styles,
            ),
            areas.status,
        );
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
    }
}

fn layout_areas(area: Rect) -> PageLayout {
    let [header, editor, status] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .flex(Flex::Start)
    .areas(area);
    PageLayout {
        header,
        editor,
        status,
    }
}

/// Size the scratch viewport with the same geometry used by the commit page.
pub fn message_content_area(area: Rect, snapshot: Option<&EditorSnapshot>) -> Rect {
    let content = EditorComponent::content_area(layout_areas(area).editor);
    match snapshot {
        Some(snapshot) => EditorBuffer::text_area(content, snapshot),
        None => content,
    }
}

fn header_lines(props: CommitHeaderProps<'_>) -> Vec<Line<'static>> {
    let styles = props.theme.ui();
    let (count, summary) = staged_summary(props.changes, props.workspace_root);
    vec![
        Line::styled(
            format!("Commit {count} staged files"),
            styles
                .base
                .patch(styles.key_hint)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(summary, styles.base.patch(styles.file_path)),
        Line::styled(
            "Ctrl-s commit  q close  Ctrl-c discard",
            styles.base.patch(styles.action_hint),
        ),
        Line::styled(props.notice.unwrap_or_default().to_string(), styles.base),
    ]
}

fn staged_summary(changes: &ChangesState, workspace_root: Option<&Path>) -> (usize, String) {
    let ChangesState::Ready(changes) = changes else {
        return (0, "Staged changes unavailable".to_string());
    };
    let staged = changes
        .iter()
        .filter(|change| change.staged)
        .collect::<Vec<_>>();
    let summary = staged
        .iter()
        .map(|change| change_summary(change, workspace_root))
        .collect::<Vec<_>>()
        .join("  ");
    (staged.len(), summary)
}

fn change_summary(change: &FileChange, workspace_root: Option<&Path>) -> String {
    let marker = match change.kind {
        ChangeKind::Added => "A",
        ChangeKind::Untracked => "?",
        ChangeKind::Modified => "M",
        ChangeKind::Conflict => "C",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
    };
    let path = workspace_root
        .and_then(|root| change.path.strip_prefix(root).ok())
        .unwrap_or(&change.path)
        .to_string_lossy()
        .into_owned();
    format!("{marker} {path}")
}

#[cfg(test)]
mod tests {
    use editor::DocumentFactory;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::data_object::Workspace;

    #[test]
    fn staged_summary_uses_explicit_inputs() {
        let changes = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: true,
            path: std::path::PathBuf::from("/workspace/note.txt"),
            previous_path: None,
        }]);

        assert_eq!(
            staged_summary(&changes, Some(Path::new("/workspace"))),
            (1, "M note.txt".to_string())
        );
    }

    #[test]
    fn renders_staged_summary_hints_message_and_editor_mode() {
        let root = std::path::PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes.state = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: true,
            path: root.join("note.txt"),
            previous_path: None,
        }]);
        model
            .vcs
            .commit
            .editor
            .insert_document(DocumentFactory::new().create("subject\n\nbody"));
        model.vcs.commit.editor.enter_insert_mode();
        model.vcs.commit.editor.resize_active_view(60, 8);
        model.vcs.notice = Some("Draft preserved".to_string());
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| VcsCommit.view(&model, frame))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Commit 1 staged files"));
        assert!(content.contains("M note.txt"));
        assert!(content.contains("Ctrl-s commit"));
        assert!(content.contains("Draft preserved"));
        assert!(content.contains("subject"));
        assert!(content.contains("body"));
        assert!(content.contains("INSERT"));
        let file_summary = &terminal.backend().buffer()[(0, 1)];
        assert_eq!(file_summary.symbol(), "M");
        assert_eq!(
            file_summary.fg,
            model.theme.ui().file_path.fg.unwrap(),
            "staged file names should use the semantic file-path color"
        );
        assert!(!file_summary.modifier.contains(Modifier::DIM));
    }
}
