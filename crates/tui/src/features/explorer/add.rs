use std::{
    fs::{self, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};

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
    document,
    manager::{
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        text_input_registry::TextInputRegistry,
    },
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
    path: String,
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
        self.path = prefill;
    }

    fn close(&mut self) {
        self.phase = AddPhase::Closed;
        self.path.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    fn insert(&mut self, character: char) {
        if !self.is_editing() {
            return;
        }
        self.path.push(character);
    }

    fn backspace(&mut self) {
        if !self.is_editing() {
            return;
        }
        self.path.pop();
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AddTarget {
    relative: PathBuf,
    directory: bool,
}

impl AddTarget {
    fn parse(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err("Enter a workspace-relative path".to_string());
        }
        let path = Path::new(input);
        if path.is_absolute() {
            return Err("Use a path relative to the workspace".to_string());
        }

        let mut relative = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => relative.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err("The path must stay within the workspace".to_string());
                }
            }
        }
        if relative.as_os_str().is_empty() {
            return Err("Enter a workspace-relative path".to_string());
        }
        Ok(Self {
            relative,
            directory: input.ends_with('/'),
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

pub(super) fn backspace(model: &mut Model) -> Option<Effect> {
    model.explorer.add.backspace();
    None
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::ExplorerAdd, insert_char)
}

fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    model.explorer.add.insert(character);
    None
}

pub(super) fn submit(model: &mut Model) -> Option<Effect> {
    if !model.explorer.add.is_editing() {
        return None;
    }
    let target = match AddTarget::parse(&model.explorer.add.path) {
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
    target: AddTarget,
    revision: u64,
}

impl FeatureEffect for AddRequest {
    fn perform(self: Box<Self>) -> Message {
        let relative = self.target.relative.clone();
        let result = create(&self.root, &self.target)
            .map_err(|error| format!("failed to create `{}`: {error}", relative.display()));
        Message::FeatureCompletion(FeatureCompletion::new(AddCompletion {
            revision: self.revision,
            directory: self.target.directory,
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

fn create(root: &Path, target: &AddTarget) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let parent = target
        .relative
        .parent()
        .unwrap_or(Path::new(""))
        .iter()
        .try_fold(root.clone(), |parent, name| {
            ensure_parent_directory(parent.join(name), &root)
        })?;
    let name = target.relative.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a file or directory name is required",
        )
    })?;
    let path = parent.join(name);
    if target.directory {
        fs::create_dir(&path)?;
    } else {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
    }
    fs::canonicalize(path)
}

fn ensure_parent_directory(path: PathBuf, root: &Path) -> io::Result<PathBuf> {
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let path = fs::canonicalize(path)?;
    if !path.starts_with(root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the parent directory is outside the workspace",
        ));
    }
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "the parent path is not a directory",
        ));
    }
    Ok(path)
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
    TextInput::new(&state.path, model.theme.ui()).cursor_position(input)
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
        TextInput::new(&self.state.path, self.styles).render(input, buf);
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
                    .update(model, Message::TextInput(character))
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
    fn targets_are_relative_and_trailing_slash_selects_directories() {
        assert_eq!(
            AddTarget::parse("folder/new/").unwrap(),
            AddTarget {
                relative: PathBuf::from("folder/new"),
                directory: true,
            }
        );
        assert_eq!(
            AddTarget::parse("./note.txt").unwrap(),
            AddTarget {
                relative: PathBuf::from("note.txt"),
                directory: false,
            }
        );
        for invalid in ["", ".", "/tmp/outside", "../outside", "folder/../outside"] {
            assert!(AddTarget::parse(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn creation_creates_missing_parent_directories() {
        let workspace = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        let file = AddTarget::parse("crates/tui/src/test1/test2").unwrap();

        let created = create(&root, &file).unwrap();
        assert_eq!(created, root.join("crates/tui/src/test1/test2"));
        assert!(created.is_file());

        let directory = AddTarget::parse("another/nested/folder/").unwrap();
        let created = create(&root, &directory).unwrap();
        assert_eq!(created, root.join("another/nested/folder"));
        assert!(created.is_dir());
    }

    #[test]
    fn creation_does_not_overwrite_or_follow_a_parent_symlink_outside() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(workspace.path()).unwrap();
        fs::write(root.join("note.txt"), "keep").unwrap();
        let existing = AddTarget::parse("note.txt").unwrap();
        assert!(create(&root, &existing).is_err());
        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "keep");

        std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();
        let escape = AddTarget::parse("linked/escape.txt").unwrap();
        let error = create(&root, &escape).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!outside.path().join("escape.txt").exists());
    }

    #[test]
    fn file_creation_prefills_from_file_opens_buffer_and_reveals_entry() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("folder")).unwrap();
        fs::write(directory.path().join("folder/existing.txt"), "old").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let root = model.workspace.as_ref().unwrap().root().to_path_buf();
        assert!(model.explorer.unfold_selected(&root).unwrap());
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
        assert_eq!(model.explorer.add.path, "folder/");
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
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));

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
        assert_eq!(model.explorer.add.path, "folder/");
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
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
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
        assert_eq!(model.explorer.add.path, "../outside");

        while !model.explorer.add.path.is_empty() {
            manager.update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_add_backspace")),
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
        assert_eq!(model.explorer.add.path, "note.txt");
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
