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
        text_input::{TextInput, TextInputChange, TextInputEvent},
    },
    data_object::{Context, Message, Model, StatusMessage},
    document,
    filesystem::{self, CreateTarget},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    theme::{Theme, UiTheme},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::{ExplorerEntry, ExplorerEntryKind};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AddPhase {
    #[default]
    Closed,
    Editing,
    Creating,
}

#[derive(Debug, Default)]
pub(crate) struct AddState {
    phase: AddPhase,
    path: TextInput,
    revision: u64,
}

impl AddState {
    fn is_open(&self) -> bool {
        self.phase != AddPhase::Closed
    }

    fn is_editing(&self) -> bool {
        self.phase == AddPhase::Editing
    }

    fn is_creating(&self) -> bool {
        self.phase == AddPhase::Creating
    }

    fn open(&mut self, prefill: String) {
        self.phase = AddPhase::Editing;
        self.path.set_value(prefill);
    }

    fn close(&mut self) {
        self.phase = AddPhase::Closed;
        self.path.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    fn handle_event(&mut self, event: TextInputEvent) -> Option<TextInputChange> {
        self.is_editing().then(|| self.path.handle_event(event))
    }

    fn begin_create(&mut self) -> Option<u64> {
        if !self.is_editing() {
            return None;
        }
        self.phase = AddPhase::Creating;
        self.revision = self.revision.wrapping_add(1);
        Some(self.revision)
    }

    fn accepts_completion(&self, revision: u64) -> bool {
        self.is_creating() && self.revision == revision
    }

    fn fail(&mut self, revision: u64) -> bool {
        if !self.accepts_completion(revision) {
            return false;
        }
        self.phase = AddPhase::Editing;
        true
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
    if model.explorer.add.is_open() || !model.modal.open(Context::ExplorerAdd) {
        return None;
    }

    let prefill = prefill(&root, model.explorer.selected_entry());
    model.explorer.add.open(prefill);
    model.context_stack.push(Context::ExplorerAdd);
    None
}

pub(super) fn cancel(model: &mut Model) -> Option<Effect> {
    if model.explorer.add.is_creating() {
        return None;
    }
    close_prompt(model);
    None
}

pub(crate) fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    model.explorer.add.handle_event(event);
    None
}

pub(super) fn submit(model: &mut Model) -> Option<Effect> {
    if !model.explorer.add.is_editing() {
        return None;
    }
    let target = match CreateTarget::parse(&model.explorer.add.path) {
        Ok(target) => target,
        Err(error) => {
            return notify(model, StatusMessage::error(error));
        }
    };
    let Some(root) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
    else {
        return notify(model, StatusMessage::error("No folder open"));
    };
    let revision = model.explorer.add.begin_create()?;
    Some(Effect::feature(AddRequest {
        root,
        target,
        revision,
    }))
}

fn close_prompt(model: &mut Model) {
    model.explorer.add.close();
    model.modal.close(Context::ExplorerAdd);
    model
        .context_stack
        .pop_if(|context| context == &Context::ExplorerAdd);
}

fn prefill(root: &Path, selected: Option<&ExplorerEntry>) -> String {
    let directory = match selected {
        Some(entry) if entry.kind == ExplorerEntryKind::Directory => entry.path.as_path(),
        Some(entry) => entry.path.parent().unwrap_or(root),
        None => root,
    };
    let relative = directory.strip_prefix(root).unwrap_or(Path::new(""));
    if relative.as_os_str().is_empty() {
        String::new()
    } else {
        format!("{}/", relative.to_string_lossy())
    }
}

struct AddRequest {
    root: PathBuf,
    target: CreateTarget,
    revision: u64,
}

impl FeatureEffect for AddRequest {
    fn perform(self: Box<Self>) -> Message {
        let relative = self.target.relative().to_path_buf();
        let result = filesystem::create(&self.root, &self.target)
            .map_err(|error| format!("failed to create `{}`: {error}", relative.display()));
        Message::FeatureCompletion(FeatureCompletion::new(AddCompletion {
            revision: self.revision,
            directory: self.target.is_directory(),
            result,
        }))
    }
}

struct AddCompletion {
    revision: u64,
    directory: bool,
    result: Result<PathBuf, String>,
}

impl Completion for AddCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match self.result {
            Ok(path) if model.explorer.add.accepts_completion(self.revision) => {
                close_prompt(model);
                if self.directory {
                    let root = model.workspace.as_ref()?.root().to_path_buf();
                    model
                        .explorer
                        .reveal_request(&root, &path)
                        .map(Effect::feature)
                } else {
                    let documents = model.explorer.documents().clone();
                    document::open_file(&documents, model, &path)
                }
            }
            Ok(_) => None,
            Err(error) => {
                if !model.explorer.add.fail(self.revision) {
                    return None;
                }
                notify(model, StatusMessage::error(error))
            }
        }
    }
}

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    if !model.explorer.add.is_open() {
        return None;
    }
    let height = if model.explorer.add.is_creating() {
        6
    } else {
        5
    };
    Some(
        OverlayContribution::new(
            "explorer_add",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(64, height)),
            render,
        )
        .with_cursor(cursor),
    )
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(
        AddPrompt {
            state: &model.explorer.add,
            theme: &model.theme,
        },
        area,
    );
}

fn cursor(model: &Model, area: Rect) -> Option<Position> {
    let state = &model.explorer.add;
    if !state.is_editing() {
        return None;
    }
    let (input, _) = content_areas(popup::content_area(area));
    state.path.cursor_position(input, model.theme.ui())
}

struct AddPrompt<'a> {
    state: &'a AddState,
    theme: &'a Theme,
}

struct AddContent<'a> {
    state: &'a AddState,
    styles: &'a UiTheme,
}

impl Widget for AddPrompt<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new(
            "Add file or directory",
            AddContent {
                state: self.state,
                styles: self.theme.ui(),
            },
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

impl Widget for AddContent<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (input, status_area) = content_areas(area);
        let surface = self.styles.popup_surface();
        self.state.path.view(self.styles).render(input, buf);
        if self.state.is_creating() {
            let status = Line::styled("Creating…", surface.patch(self.styles.muted));
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
        data_object::CommandId, explorer::ExpansionState, manager::message_manager::MessageManager,
        startup::initial_model,
    };

    fn entry(root: &Path, relative: &str, kind: ExplorerEntryKind) -> ExplorerEntry {
        let path = root.join(relative);
        ExplorerEntry {
            name: path.file_name().unwrap().to_os_string(),
            path,
            kind,
            depth: 0,
            expansion: ExpansionState::Collapsed,
        }
    }

    fn type_path(manager: &MessageManager, model: &mut Model, path: &str) {
        for character in path.chars() {
            assert!(
                manager
                    .update(model, Message::TextInput(TextInputEvent::Insert(character)))
                    .is_none()
            );
        }
    }

    fn complete_feature(
        manager: &MessageManager,
        model: &mut Model,
        effect: Effect,
    ) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected a filesystem or explorer effect");
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
    fn prefill_uses_the_selected_directory_or_files_parent() {
        let root = Path::new("/workspace");
        let folder = entry(root, "folder", ExplorerEntryKind::Directory);
        let nested = entry(root, "folder/nested", ExplorerEntryKind::Directory);
        let file = entry(root, "folder/note.txt", ExplorerEntryKind::File);
        let root_file = entry(root, "note.txt", ExplorerEntryKind::File);

        assert_eq!(prefill(root, Some(&folder)), "folder/");
        assert_eq!(prefill(root, Some(&nested)), "folder/nested/");
        assert_eq!(prefill(root, Some(&file)), "folder/");
        assert_eq!(prefill(root, Some(&root_file)), "");
        assert_eq!(prefill(root, None), "");
    }

    #[test]
    fn file_creation_prefills_from_file_opens_buffer_and_reveals_entry() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("folder")).unwrap();
        fs::write(directory.path().join("folder/existing.txt"), "old").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let root = model.workspace.as_ref().unwrap().root().to_path_buf();
        assert_eq!(
            model.explorer.unfold_selected(&root).unwrap(),
            crate::explorer::TreeChange::Changed
        );
        model.explorer.selected = model
            .explorer
            .entries
            .iter()
            .position(|entry| entry.path == root.join("folder/existing.txt"));
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_add")),
        );
        assert_eq!(model.explorer.add.path.value(), "folder/");
        assert_eq!(model.context_stack.last(), Some(&Context::ExplorerAdd));
        type_path(&manager, &mut model, "new.txt");
        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_add_submit")),
            )
            .unwrap();
        let open = complete_feature(&manager, &mut model, effect).unwrap();
        let created = root.join("folder/new.txt");
        assert_eq!(fs::read_to_string(&created).unwrap(), "");
        assert!(!model.explorer.add.is_open());
        assert!(model.context_stack.is_empty());

        let Effect::OpenDocument(request) = open else {
            panic!("expected document-open effect");
        };
        let reveal = manager.update(&mut model, Message::FileOpened(request.perform()));
        if let Some(effect) = reveal {
            apply_features(&manager, &mut model, effect);
        }
        let active = model.editor.active_buffer().unwrap();
        assert_eq!(model.editor.buffer_path(active), Some(created.as_path()));
        assert_eq!(model.explorer.selected_entry().unwrap().path, created);
    }

    #[test]
    fn directory_creation_prefills_from_folder_and_reveals_without_opening() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("folder")).unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let root = model.workspace.as_ref().unwrap().root().to_path_buf();
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_add")),
        );
        assert_eq!(model.explorer.add.path.value(), "folder/");
        type_path(&manager, &mut model, "new/");
        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_add_submit")),
            )
            .unwrap();
        let reveal = complete_feature(&manager, &mut model, effect).unwrap();
        let created = root.join("folder/new");
        assert!(created.is_dir());
        assert!(model.editor.is_empty());
        apply_features(&manager, &mut model, reveal);
        assert_eq!(model.explorer.selected_entry().unwrap().path, created);
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn invalid_and_failed_paths_notify_and_keep_the_prompt_open() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("note.txt"), "keep").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_add")),
        );

        type_path(&manager, &mut model, "../outside");
        let notification = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_add_submit")),
        );
        assert!(notification.is_some());
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "The path must stay within the workspace"
        );
        assert!(model.explorer.add.is_editing());
        assert_eq!(model.explorer.add.path.value(), "../outside");

        while !model.explorer.add.path.is_empty() {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::DeleteBackward),
            );
        }
        type_path(&manager, &mut model, "note.txt");
        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_add_submit")),
            )
            .unwrap();
        let notification = complete_feature(&manager, &mut model, effect);
        assert!(notification.is_some());
        assert!(model.explorer.add.is_editing());
        assert_eq!(model.explorer.add.path.value(), "note.txt");
        assert!(
            model
                .notifications
                .current()
                .unwrap()
                .message()
                .contains("failed to create `note.txt`")
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("note.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn prompt_renders_the_prefill_without_common_key_hints() {
        let mut model = Model::default();
        assert!(overlay(&model).is_none());
        model.explorer.add.open("folder/".to_string());
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
        assert!(content.contains("Add file or directory"));
        assert!(content.contains("folder/"));
        assert!(!content.contains("Enter create"));
        assert!(!content.contains("path/ creates directory"));
        assert!(cursor(&model, Rect::new(0, 0, 64, 5)).is_some());
    }
}
