use std::collections::HashMap;

use color_eyre::eyre::eyre;

use crate::{
    component::text_input::TextInputEvent,
    data_object::{Context, Model},
    manager::{effect::Effect, input_context::input_context},
};

pub type TextInsertHandler = fn(&mut Model, char) -> Option<Effect>;
pub type TextEventHandler = fn(&mut Model, TextInputEvent) -> Option<Effect>;

#[derive(Debug, Clone, Copy)]
pub enum TextInputHandler {
    Insert(TextInsertHandler),
    Edit(TextEventHandler),
}

/// Routes semantic text input to the feature that owns the active context.
///
/// Character-only handlers implement prefix parsers. Edit handlers accept insertion,
/// backward deletion, and cursor movement through the shared text-input component.
#[derive(Debug, Default, Clone)]
pub struct TextInputRegistry {
    handlers: HashMap<Context, TextInputHandler>,
}

impl TextInputRegistry {
    pub fn builtin() -> Self {
        use TextInputHandler::{Edit, Insert};

        Self::from_handlers([
            (Context::AppNormal, Insert(crate::line_jump::handle_input)),
            (Context::EditorInput, Insert(crate::editing::insert_char)),
            (Context::VcsCommitInput, Insert(crate::editing::insert_char)),
            (Context::FilePicker, Edit(crate::file_picker::handle_input)),
            (Context::Explorer, Insert(crate::line_jump::handle_input)),
            (
                Context::ExplorerAdd,
                Edit(crate::explorer::handle_add_input),
            ),
            (
                Context::ExplorerTransfer,
                Edit(crate::explorer::handle_transfer_input),
            ),
            (
                Context::ThemePicker,
                Edit(crate::theme_picker::handle_input),
            ),
            (
                Context::BufferJump,
                Insert(crate::buffer_jump::handle_input),
            ),
            (Context::BufferGrep, Edit(crate::buffer_grep::handle_input)),
            (
                Context::BufferNavigation,
                Insert(crate::buffer_navigation::handle_input),
            ),
            (
                Context::ProjectGrep,
                Edit(crate::project_grep::handle_input),
            ),
            (Context::LineJump, Insert(crate::line_jump::handle_input)),
            (Context::ViewAlign, Insert(crate::view_align::handle_input)),
            (Context::EditorChange, Insert(crate::change::handle_input)),
            (Context::EditorYank, Insert(crate::yank::handle_input)),
            (Context::VcsHistory, Insert(crate::line_jump::handle_input)),
            (
                Context::EditorVisual,
                Insert(crate::line_jump::handle_input),
            ),
            (Context::VcsMerge, Edit(crate::vcs::merge::handle_input)),
            (Context::VcsReset, Edit(crate::vcs::reset::handle_input)),
            (Context::VcsBranch, Edit(crate::vcs::branch::handle_input)),
            (Context::AgentChatDialog, Edit(crate::agent::handle_input)),
            (Context::AgentChatInsert, Edit(crate::agent::handle_input)),
            (
                Context::ProviderLogin,
                Edit(crate::provider_login::handle_input),
            ),
        ])
        .expect("built-in text-input contexts must be unique")
    }

    fn from_handlers(
        handlers: impl IntoIterator<Item = (Context, TextInputHandler)>,
    ) -> color_eyre::Result<Self> {
        let mut registry = Self::default();
        for (context, handler) in handlers {
            if registry.handlers.contains_key(&context) {
                return Err(eyre!("duplicate text-input handler for `{context:?}`"));
            }
            registry.handlers.insert(context, handler);
        }
        Ok(registry)
    }

    pub fn accepts(&self, context: &Context, event: TextInputEvent) -> bool {
        matches!(
            (self.handlers.get(context), event),
            (Some(TextInputHandler::Insert(_)), TextInputEvent::Insert(_))
                | (Some(TextInputHandler::Edit(_)), _)
        )
    }

    pub fn handle(&self, model: &mut Model, event: TextInputEvent) -> Option<Effect> {
        let context = input_context(model)?;
        match (self.handlers.get(&context)?, event) {
            (TextInputHandler::Insert(handler), TextInputEvent::Insert(character)) => {
                handler(model, character)
            }
            (TextInputHandler::Edit(handler), event) => handler(model, event),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::text_input::TextCursorMovement;

    fn replacement_input(_: &mut Model, _: char) -> Option<Effect> {
        None
    }

    #[test]
    fn rejects_duplicate_context_handlers() {
        let result = TextInputRegistry::from_handlers([
            (
                Context::EditorInput,
                TextInputHandler::Insert(crate::editing::insert_char),
            ),
            (
                Context::EditorInput,
                TextInputHandler::Insert(replacement_input),
            ),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn capabilities_follow_the_handler_kind() {
        let registry = TextInputRegistry::builtin();
        let edit_events = [
            TextInputEvent::Insert('x'),
            TextInputEvent::DeleteBackward,
            TextInputEvent::Cursor(TextCursorMovement::Left),
        ];
        [
            Context::FilePicker,
            Context::ThemePicker,
            Context::BufferGrep,
            Context::ProjectGrep,
            Context::ExplorerAdd,
            Context::ExplorerTransfer,
            Context::AgentChatDialog,
            Context::AgentChatInsert,
            Context::VcsMerge,
            Context::VcsReset,
            Context::VcsBranch,
        ]
        .into_iter()
        .for_each(|context| {
            for event in edit_events {
                assert!(registry.accepts(&context, event), "{context:?}: {event:?}");
            }
        });

        [
            Context::EditorInput,
            Context::VcsCommitInput,
            Context::BufferJump,
        ]
        .into_iter()
        .for_each(|context| {
            assert!(registry.accepts(&context, TextInputEvent::Insert('x')));
            assert!(!registry.accepts(&context, TextInputEvent::DeleteBackward));
            assert!(!registry.accepts(&context, TextInputEvent::Cursor(TextCursorMovement::Left)));
        });
        assert!(!registry.accepts(&Context::AgentApproval, TextInputEvent::Insert('x')));
    }

    #[test]
    fn dispatches_to_a_derived_agent_insert_context() {
        let registry = TextInputRegistry::builtin();
        let mut model = Model {
            current_focus: crate::data_object::FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(crate::tools::ToolKind::Agent);
        model.tools.visibility = crate::data_object::Visibility::Displayed;
        model.agent.enter_insert_mode();

        registry.handle(&mut model, TextInputEvent::Insert('x'));
        assert_eq!(model.agent.prompt(), "x");
        assert!(model.context_stack.is_empty());
    }
}
