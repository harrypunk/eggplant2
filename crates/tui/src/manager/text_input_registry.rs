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
            .register(Context::FilePicker, crate::file_picker::insert_char)
            .expect("file-picker text input must be unique");
        crate::buffer_jump::register_text_input(&mut registry)
            .expect("buffer-jump text input must be unique");
        crate::buffer_grep::register_text_input(&mut registry)
            .expect("buffer-grep text input must be unique");
        registry
    }

    pub fn register(
        &mut self,
        context: Context,
        handler: TextInputHandler,
    ) -> color_eyre::Result<()> {
        if self.handlers.insert(context, handler).is_some() {
            return Err(eyre!("duplicate text-input handler for `{context:?}`"));
        }
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
