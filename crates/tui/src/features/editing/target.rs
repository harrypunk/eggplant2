use editor::EditorSession;

use crate::{
    buffer,
    data_object::{AppPage, Context, Model},
    manager::effect::Effect,
};

/// Page-independent capabilities required by reusable editing commands.
pub(crate) trait EditingTarget {
    fn session(&mut self) -> &mut EditorSession;
    fn normal_context(&self) -> Context {
        Context::AppNormal
    }
    fn input_context(&self) -> Context;
    fn push_context(&mut self, context: Context);
    fn pop_context(&mut self, context: Context);

    fn visual_context(&self) -> Context {
        Context::EditorVisual
    }

    fn is_editing_enabled(&self) -> bool {
        true
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

    fn input_context(&self) -> Context {
        Context::EditorInput
    }

    fn push_context(&mut self, context: Context) {
        self.model.context_stack.push(context);
    }

    fn pop_context(&mut self, context: Context) {
        self.model.context_stack.pop_if(|active| *active == context);
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

    fn normal_context(&self) -> Context {
        Context::VcsCommit
    }

    fn input_context(&self) -> Context {
        Context::VcsCommitInput
    }

    fn push_context(&mut self, context: Context) {
        self.model.context_stack.push(context);
    }

    fn pop_context(&mut self, context: Context) {
        self.model.context_stack.pop_if(|active| *active == context);
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
