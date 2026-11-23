use std::collections::HashMap;

use color_eyre::eyre::eyre;

use crate::{
    component::text_input::{TextCursorMovement, TextInputEvent},
    data_object::{Context, Model},
    manager::effect::Effect,
};

pub type TextInsertHandler = fn(&mut Model, char) -> Option<Effect>;
pub type TextEventHandler = fn(&mut Model, TextInputEvent) -> Option<Effect>;

#[derive(Debug, Clone, Copy)]
pub enum TextInputHandler {
    Insert(TextInsertHandler),
    Edit(TextEventHandler),
}

/// Routes unbound printable input to the feature that owns the active context.
#[derive(Debug, Default, Clone)]
pub struct TextInputRegistry {
    handlers: HashMap<Context, TextInputHandler>,
}

impl TextInputRegistry {
    pub fn builtin() -> Self {
        let mut registry = Self::default();
        registry
            .register(Context::EditorInput, crate::editing::insert_char)
            .expect("editor text input must be unique");
        registry
            .register(Context::VcsCommitInput, crate::editing::insert_char)
            .expect("commit-message text input must be unique");
        registry
            .register_with_cursor(Context::FilePicker, crate::file_picker::handle_input)
            .expect("file-picker text input must be unique");
        crate::explorer::register_text_input(&mut registry)
            .expect("explorer-add text input must be unique");
        crate::theme_picker::register_text_input(&mut registry)
            .expect("theme-picker text input must be unique");
        crate::buffer_jump::register_text_input(&mut registry)
            .expect("buffer-jump text input must be unique");
        crate::buffer_grep::register_text_input(&mut registry)
            .expect("buffer-grep text input must be unique");
        crate::buffer_navigation::register_text_input(&mut registry)
            .expect("buffer-navigation text input must be unique");
        crate::project_grep::register_text_input(&mut registry)
            .expect("project-grep text input must be unique");
        crate::line_jump::register_text_input(&mut registry)
            .expect("line-jump text input must be unique");
        crate::view_align::register_text_input(&mut registry)
            .expect("view-alignment text input must be unique");
        crate::change::register_text_input(&mut registry)
            .expect("change text input must be unique");
        crate::yank::register_text_input(&mut registry).expect("yank text input must be unique");
        crate::vcs::merge::register_text_input(&mut registry)
            .expect("merge-target text input must be unique");
        crate::vcs::reset::register_text_input(&mut registry)
            .expect("reset-target text input must be unique");
        crate::vcs::branch::register_text_input(&mut registry)
            .expect("branch text input must be unique");
        crate::agent::register_text_input(&mut registry).expect("agent text input must be unique");
        registry
    }

    pub fn register(
        &mut self,
        context: Context,
        handler: TextInsertHandler,
    ) -> color_eyre::Result<()> {
        self.register_handler(context, TextInputHandler::Insert(handler))
    }

    pub fn register_with_cursor(
        &mut self,
        context: Context,
        handler: TextEventHandler,
    ) -> color_eyre::Result<()> {
        self.register_handler(context, TextInputHandler::Edit(handler))
    }

    fn register_handler(
        &mut self,
        context: Context,
        handler: TextInputHandler,
    ) -> color_eyre::Result<()> {
        if self.handlers.contains_key(&context) {
            return Err(eyre!("duplicate text-input handler for `{context:?}`"));
        }
        self.handlers.insert(context, handler);
        Ok(())
    }

    pub fn accepts(&self, context: &Context) -> bool {
        self.handlers.contains_key(context)
    }

    pub fn accepts_cursor(&self, context: &Context) -> bool {
        self.handlers
            .get(context)
            .is_some_and(|handler| matches!(handler, TextInputHandler::Edit(_)))
    }

    pub fn handle(&self, model: &mut Model, character: char) -> Option<Effect> {
        let context = model.context_stack.last()?;
        match self.handlers.get(context)? {
            TextInputHandler::Insert(handler) => handler(model, character),
            TextInputHandler::Edit(handler) => handler(model, TextInputEvent::Character(character)),
        }
    }

    pub fn handle_cursor(&self, model: &mut Model, movement: TextCursorMovement) -> Option<Effect> {
        let context = model.context_stack.last()?;
        let TextInputHandler::Edit(handler) = self.handlers.get(context)? else {
            return None;
        };
        handler(model, TextInputEvent::Cursor(movement))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacement_input(_: &mut Model, _: char) -> Option<Effect> {
        None
    }

    #[test]
    fn duplicate_input_preserves_the_original_handler() {
        let mut registry = TextInputRegistry::default();
        registry
            .register(Context::EditorInput, crate::editing::insert_char)
            .unwrap();
        let original = match registry.handlers.get(&Context::EditorInput) {
            Some(TextInputHandler::Insert(handler)) => *handler as usize,
            _ => panic!("expected character handler"),
        };

        assert!(
            registry
                .register(Context::EditorInput, replacement_input)
                .is_err()
        );
        assert!(matches!(
            registry.handlers.get(&Context::EditorInput),
            Some(TextInputHandler::Insert(handler)) if *handler as usize == original
        ));
    }

    #[test]
    fn every_component_input_accepts_cursor_movement() {
        let registry = TextInputRegistry::builtin();
        [
            Context::FilePicker,
            Context::ThemePicker,
            Context::BufferGrep,
            Context::ProjectGrep,
            Context::ExplorerAdd,
            Context::ExplorerTransfer,
            Context::AgentChatDialog,
            Context::AgentChat,
            Context::AgentChatInsert,
            Context::VcsMerge,
            Context::VcsReset,
            Context::VcsBranch,
        ]
        .into_iter()
        .for_each(|context| {
            assert!(registry.accepts_cursor(&context), "{context:?}");
        });

        [
            Context::EditorInput,
            Context::VcsCommitInput,
            Context::AgentApproval,
        ]
        .into_iter()
        .for_each(|context| assert!(!registry.accepts_cursor(&context), "{context:?}"));
    }

    #[test]
    fn rejects_duplicate_context_handlers() {
        let mut registry = TextInputRegistry::default();
        registry
            .register(Context::EditorInput, crate::editing::insert_char)
            .unwrap();
        assert!(
            registry
                .register(Context::EditorInput, crate::editing::insert_char)
                .is_err()
        );
    }
}
