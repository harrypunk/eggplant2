use editor::EditorSession;

use crate::{
    buffer,
    data_object::{Context, Model},
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

    /// Register policy applied to deleted text.
    fn record_deleted(&mut self, _text: String) -> Option<Effect> {
        None
    }

    /// Persistence policy applied after a successful text mutation.
    fn after_edit(&mut self) -> Option<Effect>;
}

pub(crate) fn main_editor(model: &mut Model) -> impl EditingTarget + '_ {
    MainEditingTarget { model }
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

    fn record_deleted(&mut self, text: String) -> Option<Effect> {
        self.model.register.record(text)
    }

    fn after_edit(&mut self) -> Option<Effect> {
        buffer::auto_save_active(self.model)
    }
}

pub(crate) fn commit_message(model: &mut Model) -> impl EditingTarget + '_ {
    CommitEditingTarget { model }
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

    fn after_edit(&mut self) -> Option<Effect> {
        None
    }
}
