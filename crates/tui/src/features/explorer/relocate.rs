use std::path::{Path, PathBuf};

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect, Size},
    text::Line,
    widgets::{Paragraph, Widget},
};

use crate::{
    component::{
        popup::{self, Popup},
        text_input::TextInput,
    },
    data_object::{Context, Message, Model, StatusMessage},
    filesystem::{self, FileKind, MoveDestination},
    manager::{
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        text_input_registry::TextInputRegistry,
    },
    theme::{Theme, UiTheme},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::{ExplorerEntry, ExplorerEntryKind, buffers};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum MovePhase {
    #[default]
    Closed,
    Editing,
    Moving,
}

#[derive(Debug, Default)]
pub(crate) struct MoveState {
    phase: MovePhase,
    source: Option<MoveSource>,
    destination: String,
    revision: u64,
}

impl MoveState {
    fn is_open(&self) -> bool {
        self.phase != MovePhase::Closed
    }

    fn is_editing(&self) -> bool {
        self.phase == MovePhase::Editing
    }

    fn is_moving(&self) -> bool {
        self.phase == MovePhase::Moving
    }

    fn open(&mut self, source: MoveSource) {
        self.destination = source.relative.to_string_lossy().into_owned();
        self.source = Some(source);
        self.phase = MovePhase::Editing;
    }

    fn close(&mut self) {
        self.phase = MovePhase::Closed;
        self.source = None;
        self.destination.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    fn insert(&mut self, character: char) {
        if self.is_editing() {
            self.destination.push(character);
        }
    }

    fn backspace(&mut self) {
        if self.is_editing() {
            self.destination.pop();
        }
    }

    fn begin_move(&mut self) -> Option<u64> {
        if !self.is_editing() {
            return None;
        }
        self.phase = MovePhase::Moving;
        self.revision = self.revision.wrapping_add(1);
        Some(self.revision)
    }

    fn accepts_completion(&self, revision: u64) -> bool {
        self.is_moving() && self.revision == revision
    }

    fn fail(&mut self, revision: u64) -> bool {
        if !self.accepts_completion(revision) {
            return false;
        }
        self.phase = MovePhase::Editing;
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MoveSource {
    path: PathBuf,
    relative: PathBuf,
    kind: FileKind,
}

impl MoveSource {
    fn new(root: &Path, entry: &ExplorerEntry) -> Result<Self, String> {
        let relative = entry
            .path
            .strip_prefix(root)
            .map_err(|_| "The selected path is outside the workspace".to_string())?
            .to_path_buf();
        if relative.as_os_str().is_empty() {
            return Err("The workspace root cannot be moved".to_string());
        }
        let kind = match entry.kind {
            ExplorerEntryKind::File => FileKind::File,
            ExplorerEntryKind::Directory => FileKind::Directory,
            ExplorerEntryKind::Link => FileKind::Link,
            ExplorerEntryKind::Other => {
                return Err("The selected file type cannot be moved".to_string());
            }
        };
        Ok(Self {
            path: entry.path.clone(),
            relative,
            kind,
        })
    }
}

fn notify(model: &mut Model, status: StatusMessage) -> Option<Effect> {
    Some(crate::notifications::report_status(model, status))
}

pub(super) fn open(model: &mut Model) -> Option<Effect> {
    let Some(root) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
    else {
        return notify(model, StatusMessage::info("No folder open"));
    };
    if model.explorer.relocate.is_open() || !model.modal.open(Context::ExplorerMove) {
        return None;
    }
    let Some(entry) = model.explorer.selected_entry() else {
        model.modal.close(Context::ExplorerMove);
        return notify(model, StatusMessage::info("No file or folder selected"));
    };
    let source = match MoveSource::new(&root, entry) {
        Ok(source) => source,
        Err(error) => {
            model.modal.close(Context::ExplorerMove);
            return notify(model, StatusMessage::error(error));
        }
    };

    model.explorer.relocate.open(source);
    model.context_stack.push(Context::ExplorerMove);
    None
}

pub(super) fn cancel(model: &mut Model) -> Option<Effect> {
    if model.explorer.relocate.is_moving() {
        return None;
    }
    close_prompt(model);
    None
}

pub(super) fn backspace(model: &mut Model) -> Option<Effect> {
    model.explorer.relocate.backspace();
    None
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::ExplorerMove, insert_char)
}

fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    model.explorer.relocate.insert(character);
    None
}

pub(super) fn submit(model: &mut Model) -> Option<Effect> {
    if !model.explorer.relocate.is_editing() {
        return None;
    }
    let destination = match MoveDestination::parse(&model.explorer.relocate.destination) {
        Ok(destination) => destination,
        Err(error) => return notify(model, StatusMessage::error(error)),
    };
    let Some(root) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
    else {
        return notify(model, StatusMessage::error("No folder open"));
    };
    let source = model.explorer.relocate.source.clone()?;
    if let Some(effect) = buffers::reject_blocked(model, &source.path, source.kind, "moving") {
        return Some(effect);
    }
    let revision = model.explorer.relocate.begin_move()?;
    Some(Effect::feature(MoveRequest {
        root,
        source,
        destination,
        revision,
    }))
}

fn close_prompt(model: &mut Model) {
    model.explorer.relocate.close();
    model.modal.close(Context::ExplorerMove);
    model
        .context_stack
        .pop_if(|context| context == &Context::ExplorerMove);
}

struct MoveRequest {
    root: PathBuf,
    source: MoveSource,
    destination: MoveDestination,
    revision: u64,
}

impl FeatureEffect for MoveRequest {
    fn perform(self: Box<Self>) -> Message {
        let source_relative = self.source.relative.clone();
        let destination_relative = self.destination.relative().to_path_buf();
        let result = filesystem::relocate(
            &self.root,
            &self.source.path,
            self.source.kind,
            &self.destination,
        )
        .map_err(|error| {
            format!(
                "failed to move `{}` to `{}`: {error}",
                source_relative.display(),
                destination_relative.display()
            )
        });
        Message::FeatureCompletion(FeatureCompletion::new(MoveCompletion {
            revision: self.revision,
            source: self.source,
            result,
        }))
    }
}

struct MoveCompletion {
    revision: u64,
    source: MoveSource,
    result: Result<PathBuf, String>,
}

impl Completion for MoveCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match self.result {
            Ok(path) if model.explorer.relocate.accepts_completion(self.revision) => {
                close_prompt(model);
                let close = buffers::close(model, &self.source.path, self.source.kind);
                let root = model.workspace.as_ref()?.root().to_path_buf();
                let reveal = model
                    .explorer
                    .reveal_request(&root, &path)
                    .map(Effect::feature);
                Effect::combine(reveal, close)
            }
            Ok(_) => None,
            Err(error) => {
                if !model.explorer.relocate.fail(self.revision) {
                    return None;
                }
                notify(model, StatusMessage::error(error))
            }
        }
    }
}

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    if !model.explorer.relocate.is_open() {
        return None;
    }
    let height = if model.explorer.relocate.is_moving() {
        6
    } else {
        5
    };
    Some(
        OverlayContribution::new(
            "explorer_move",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(64, height)),
            render,
        )
        .with_cursor(cursor),
    )
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(
        MovePrompt {
            state: &model.explorer.relocate,
            theme: &model.theme,
        },
        area,
    );
}

fn cursor(model: &Model, area: Rect) -> Option<Position> {
    let state = &model.explorer.relocate;
    if !state.is_editing() {
        return None;
    }
    let (input, _) = content_areas(popup::content_area(area));
    TextInput::new(&state.destination, model.theme.ui()).cursor_position(input)
}

struct MovePrompt<'a> {
    state: &'a MoveState,
    theme: &'a Theme,
}

struct MoveContent<'a> {
    state: &'a MoveState,
    styles: &'a UiTheme,
}

impl Widget for MovePrompt<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .state
            .source
            .as_ref()
            .map(|source| format!("Move {}", source.relative.display()))
            .unwrap_or_else(|| "Move".to_string());
        Popup::new(
            title,
            MoveContent {
                state: self.state,
                styles: self.theme.ui(),
            },
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

impl Widget for MoveContent<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (input, status_area) = content_areas(area);
        let surface = self.styles.popup_surface();
        TextInput::new(&self.state.destination, self.styles).render(input, buf);
        if self.state.is_moving() {
            let status = Line::styled("Moving…", surface.patch(self.styles.muted));
            Paragraph::new(status)
                .style(surface)
                .render(status_area, buf);
        }
    }
}

fn content_areas(area: Rect) -> (Rect, Rect) {
    Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
        .areas(area)
        .into()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        data_object::CommandId, manager::message_manager::MessageManager, startup::initial_model,
    };

    fn open_move(manager: &MessageManager, model: &mut Model) {
        assert!(
            manager
                .update(model, Message::InvokeCommand(CommandId("explorer_move")))
                .is_none()
        );
        assert!(model.explorer.relocate.is_editing());
        assert_eq!(model.context_stack.last(), Some(&Context::ExplorerMove));
    }

    fn type_destination(manager: &MessageManager, model: &mut Model, destination: &str) {
        for character in destination.chars() {
            assert!(
                manager
                    .update(model, Message::TextInput(character))
                    .is_none()
            );
        }
    }

    fn replace_destination(manager: &MessageManager, model: &mut Model, destination: &str) {
        while !model.explorer.relocate.destination.is_empty() {
            manager.update(
                model,
                Message::InvokeCommand(CommandId("explorer_move_backspace")),
            );
        }
        type_destination(manager, model, destination);
    }

    fn complete_feature(
        manager: &MessageManager,
        model: &mut Model,
        effect: Effect,
    ) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected a filesystem effect");
        };
        manager.update(model, request.perform())
    }

    fn apply_features(manager: &MessageManager, model: &mut Model, effect: Effect) {
        match effect {
            Effect::Feature(request) => {
                manager.update(model, request.perform());
            }
            Effect::Batch(effects) => {
                for effect in effects {
                    apply_features(manager, model, effect);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn file_move_closes_clean_buffer_and_reveals_destination() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.txt");
        fs::write(&source, "content").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let root = model.workspace.as_ref().unwrap().root().to_path_buf();
        let buffer = crate::test_utils::open_file(&mut model, &source);
        let manager = MessageManager::default();
        open_move(&manager, &mut model);
        assert_eq!(model.explorer.relocate.destination, "source.txt");
        replace_destination(&manager, &mut model, "nested/destination.txt");

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_move_submit")),
            )
            .unwrap();
        let reveal = complete_feature(&manager, &mut model, effect).unwrap();
        let destination = root.join("nested/destination.txt");
        assert!(!source.exists());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "content");
        assert_eq!(model.editor.buffer_path(buffer), None);
        assert!(!model.explorer.relocate.is_open());
        apply_features(&manager, &mut model, reveal);
        assert_eq!(model.explorer.selected_entry().unwrap().path, destination);
    }

    #[test]
    fn directory_move_closes_nested_buffers_and_reveals_directory() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("folder");
        let nested_file = source.join("nested/note.txt");
        fs::create_dir_all(nested_file.parent().unwrap()).unwrap();
        fs::write(&nested_file, "content").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let root = model.workspace.as_ref().unwrap().root().to_path_buf();
        let buffer = crate::test_utils::open_file(&mut model, &nested_file);
        let manager = MessageManager::default();
        open_move(&manager, &mut model);
        assert_eq!(model.explorer.relocate.destination, "folder");
        replace_destination(&manager, &mut model, "renamed");

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_move_submit")),
            )
            .unwrap();
        let reveal = complete_feature(&manager, &mut model, effect).unwrap();
        let destination = root.join("renamed");
        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(destination.join("nested/note.txt")).unwrap(),
            "content"
        );
        assert_eq!(model.editor.buffer_path(buffer), None);
        apply_features(&manager, &mut model, reveal);
        assert_eq!(model.explorer.selected_entry().unwrap().path, destination);
    }

    #[test]
    fn modified_source_buffer_blocks_move_without_losing_changes() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.txt");
        fs::write(&source, "original").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = crate::test_utils::open_file(&mut model, &source);
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("changed ")
            .unwrap();
        let manager = MessageManager::default();
        open_move(&manager, &mut model);
        replace_destination(&manager, &mut model, "destination.txt");

        let notification = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_move_submit")),
        );
        assert!(notification.is_some());
        assert!(source.exists());
        assert!(!directory.path().join("destination.txt").exists());
        assert_eq!(model.editor.buffer_path(buffer), Some(source.as_path()));
        assert_eq!(
            model.editor.active_text_snapshot().unwrap().text,
            "changed original"
        );
        assert!(model.explorer.relocate.is_editing());
    }

    #[test]
    fn cancellation_preserves_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.txt");
        fs::write(&source, "content").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        open_move(&manager, &mut model);
        replace_destination(&manager, &mut model, "destination.txt");

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_explorer_move")),
        );
        assert!(source.exists());
        assert!(!directory.path().join("destination.txt").exists());
        assert!(!model.explorer.relocate.is_open());
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
    }

    #[test]
    fn existing_destination_notifies_and_retains_the_prompt() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.txt");
        let destination = directory.path().join("destination.txt");
        fs::write(&source, "source").unwrap();
        fs::write(&destination, "destination").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.explorer.selected = model
            .explorer
            .entries
            .iter()
            .position(|entry| entry.path == source);
        let manager = MessageManager::default();
        open_move(&manager, &mut model);
        replace_destination(&manager, &mut model, "destination.txt");

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_move_submit")),
            )
            .unwrap();
        let notification = complete_feature(&manager, &mut model, effect);
        assert!(notification.is_some());
        assert_eq!(fs::read_to_string(source).unwrap(), "source");
        assert_eq!(fs::read_to_string(destination).unwrap(), "destination");
        assert!(model.explorer.relocate.is_editing());
        assert_eq!(model.explorer.relocate.destination, "destination.txt");
        assert!(
            model
                .notifications
                .current()
                .unwrap()
                .message()
                .contains("the destination already exists")
        );
    }

    #[test]
    fn prompt_shows_the_source_and_editable_destination() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("source.txt"), "content").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        open_move(&manager, &mut model);
        let contribution = overlay(&model).unwrap();
        let backend = TestBackend::new(70, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| (contribution.render)(&model, frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Move source.txt"));
        assert!(content.contains("source.txt"));
        assert!(cursor(&model, Rect::new(0, 0, 64, 5)).is_some());
    }
}
