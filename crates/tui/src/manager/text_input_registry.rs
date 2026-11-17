use std::collections::HashMap;

use color_eyre::eyre::eyre;

use crate::{
    data_object::{Context, Model},
    manager::effect::Effect,
};

pub type TextInputHandler = fn(&mut Model, char) -> Option<Effect>;

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
            .register(Context::FilePicker, crate::file_picker::insert_char)
            .expect("file-picker text input must be unique");
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
        registry
    }

    pub fn register(
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

    pub fn handle(&self, model: &mut Model, character: char) -> Option<Effect> {
        let context = model.context_stack.last()?;
        let handler = self.handlers.get(context)?;
        handler(model, character)
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
        let original = registry
            .handlers
            .get(&Context::EditorInput)
            .copied()
            .unwrap() as usize;

        assert!(
            registry
                .register(Context::EditorInput, replacement_input)
                .is_err()
        );
        assert_eq!(
            registry
                .handlers
                .get(&Context::EditorInput)
                .copied()
                .unwrap() as usize,
            original
        );
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
