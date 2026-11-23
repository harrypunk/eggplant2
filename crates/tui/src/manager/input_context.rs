use crate::data_object::{AppPage, Context, FocusTarget, Model};

/// Derive the active keyboard context from source-of-truth UI and feature state.
///
/// The stored stack still owns modal/workflow contexts during migration. Base
/// editor variants are selectors, not state that lifecycle code must maintain.
pub fn input_context(model: &Model) -> Option<Context> {
    model
        .context_stack
        .last()
        .copied()
        .map(|context| match context {
            Context::AppNormal
                if model.active_page == AppPage::Editor
                    && model.current_focus == FocusTarget::Editor
                    && model.editor.active_buffer().is_none() =>
            {
                Context::Welcome
            }
            context => context,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal_model() -> Model {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model
    }

    #[test]
    fn derives_welcome_only_for_the_focused_empty_editor() {
        let mut model = normal_model();
        assert_eq!(input_context(&model), Some(Context::Welcome));

        model.current_focus = FocusTarget::FileExplorer;
        assert_eq!(input_context(&model), Some(Context::AppNormal));

        model.current_focus = FocusTarget::Editor;
        crate::test_utils::create_buffer(&mut model, "text");
        assert_eq!(input_context(&model), Some(Context::AppNormal));
    }

    #[test]
    fn stored_workflow_and_modal_contexts_take_precedence() {
        let mut model = normal_model();
        model.context_stack.push(Context::Explorer);
        assert_eq!(input_context(&model), Some(Context::Explorer));
        model.context_stack.push(Context::FilePicker);
        assert_eq!(input_context(&model), Some(Context::FilePicker));
    }

    #[test]
    fn welcome_is_not_derived_on_other_pages() {
        let mut model = normal_model();
        model.active_page = AppPage::Vcs;
        assert_eq!(input_context(&model), Some(Context::AppNormal));
    }
}
