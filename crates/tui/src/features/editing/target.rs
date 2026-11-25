use editor::{EditMode, EditorSession};

use crate::{
    buffer,
    data_object::{AppPage, Context, FocusTarget, Model},
    manager::effect::Effect,
};

/// Page-independent capabilities required by reusable editing commands.
pub(crate) trait EditingTarget {
    fn session(&mut self) -> &mut EditorSession;
    fn normal_input_available(&self) -> bool;

    fn is_editing_enabled(&self) -> bool {
        true
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn can_mutate(&self) -> bool {
        self.is_editing_enabled() && !self.is_read_only()
    }

    /// Text available for paste through the shared register policy.
    fn register_text(&self) -> String {
        String::new()
    }

    /// Register policy applied to deleted text.
    fn record_deleted(&mut self, _text: String) -> Option<Effect> {
        None
    }

    /// Explicit yanks share the register by default, like delete operations.
    fn record_yanked(&mut self, text: String) -> Option<Effect> {
        self.record_deleted(text)
    }

    /// Persistence policy applied after a successful text mutation.
    fn after_edit(&mut self) -> Option<Effect>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditingTargetKind {
    Main,
    Commit,
}

pub(crate) fn active_kind(model: &Model) -> Option<EditingTargetKind> {
    match model.active_page {
        AppPage::Editor => Some(EditingTargetKind::Main),
        AppPage::VcsCommit => Some(EditingTargetKind::Commit),
        AppPage::Vcs | AppPage::VcsHistory => None,
    }
}

/// Map an editor-owned mode to its keymap context without consulting application state.
pub(crate) fn mode_context(mode: EditMode, insert: Context) -> Option<Context> {
    match mode {
        EditMode::Insert => Some(insert),
        EditMode::Visual | EditMode::VisualLine => Some(Context::EditorVisual),
        EditMode::Normal => None,
    }
}

pub(crate) fn main_normal_input_available(model: &Model) -> bool {
    model.active_page == AppPage::Editor
        && model.current_focus == FocusTarget::Editor
        && model.editor.mode() == EditMode::Normal
        && model.editor.active_buffer().is_some()
        && model.modal.active_context().is_none()
        && model.context_stack.is_empty()
}

fn commit_normal_input_available(model: &Model) -> bool {
    model.active_page == AppPage::VcsCommit
        && model.vcs.commit.editor.mode() == EditMode::Normal
        && model.modal.active_context().is_none()
        && model.context_stack.is_empty()
}

pub(crate) fn with_active<R>(
    model: &mut Model,
    operation: impl FnOnce(&mut dyn EditingTarget) -> R,
) -> Option<R> {
    match active_kind(model)? {
        EditingTargetKind::Main => Some(operation(&mut MainEditingTarget { model })),
        EditingTargetKind::Commit => Some(operation(&mut CommitEditingTarget { model })),
    }
}

fn record_explicit_yank(model: &mut Model, text: String) -> Option<Effect> {
    let register = model.register.record(text);
    let notification = crate::notifications::publish(
        model,
        crate::capabilities::notification::Notification::info("Yanked text").with_title("Yank"),
    );
    Effect::combine(register, Some(notification))
}

struct MainEditingTarget<'a> {
    model: &'a mut Model,
}

impl EditingTarget for MainEditingTarget<'_> {
    fn session(&mut self) -> &mut EditorSession {
        &mut self.model.editor
    }

    fn normal_input_available(&self) -> bool {
        main_normal_input_available(self.model)
    }

    fn is_read_only(&self) -> bool {
        crate::capabilities::readonly::active_buffer_is_read_only(self.model)
    }

    fn register_text(&self) -> String {
        self.model.register.unnamed().to_string()
    }

    fn record_deleted(&mut self, text: String) -> Option<Effect> {
        self.model.register.record(text)
    }

    fn record_yanked(&mut self, text: String) -> Option<Effect> {
        record_explicit_yank(self.model, text)
    }

    fn after_edit(&mut self) -> Option<Effect> {
        Effect::combine(
            buffer::auto_save_active(self.model),
            crate::code_navigation::synchronize(self.model),
        )
    }
}

struct CommitEditingTarget<'a> {
    model: &'a mut Model,
}

impl EditingTarget for CommitEditingTarget<'_> {
    fn session(&mut self) -> &mut EditorSession {
        &mut self.model.vcs.commit.editor
    }

    fn normal_input_available(&self) -> bool {
        commit_normal_input_available(self.model)
    }

    fn is_editing_enabled(&self) -> bool {
        self.model.vcs.commit.is_editing()
    }

    fn register_text(&self) -> String {
        self.model.register.unnamed().to_string()
    }

    fn record_yanked(&mut self, text: String) -> Option<Effect> {
        record_explicit_yank(self.model, text)
    }

    fn after_edit(&mut self) -> Option<Effect> {
        None
    }
}
