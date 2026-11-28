use eggplant_agent::{ThinkingId, ThinkingOption};

use crate::{
    component::list::ListState,
    data_object::{Context, GeneralAction, Model, StatusMessage},
    manager::effect::Effect,
};

#[derive(Debug, Default)]
pub struct ThinkingSelectorState {
    options: ListState<ThinkingOption>,
    active: Option<ThinkingId>,
    return_context: Option<Context>,
}

impl ThinkingSelectorState {
    pub(crate) fn open(
        &mut self,
        return_context: Option<Context>,
        options: Vec<ThinkingOption>,
        active: Option<ThinkingId>,
    ) {
        let selected = active
            .as_ref()
            .and_then(|active| options.iter().position(|option| &option.id == active))
            .unwrap_or(0);
        self.options = ListState::new(options);
        self.options.select(Some(selected));
        self.active = active;
        self.return_context = return_context;
    }

    fn close(&mut self) -> Option<Context> {
        self.options = ListState::default();
        self.active = None;
        self.return_context.take()
    }

    pub fn options(&self) -> &[ThinkingOption] {
        self.options.items()
    }

    pub fn selected(&self) -> Option<usize> {
        self.options.selected()
    }

    pub fn active_label(&self) -> Option<&str> {
        let active = self.active.as_ref()?;
        self.options
            .items()
            .iter()
            .find(|option| &option.id == active)
            .map(|option| option.label.as_str())
    }

    fn selected_id(&self) -> Option<ThinkingId> {
        self.options.selected_item().map(|option| option.id.clone())
    }

    fn select_next(&mut self) {
        self.options.select_next();
    }

    fn select_previous(&mut self) {
        self.options.select_previous();
    }
}

pub(super) fn open(model: &mut Model) -> Option<Effect> {
    let options = model.agent.runtime().thinking_options();
    if options.is_empty() {
        return Some(crate::notifications::publish_status(
            model,
            StatusMessage::info("The active provider has no thinking options".to_string()),
            Some("Agent"),
        ));
    }
    let options = options.to_vec();
    let active = model.agent.runtime().thinking_id();
    let return_context = super::dialog::suspend_for_overlay(model);
    if !model.modal.open(Context::AgentThinkingSelector) {
        super::dialog::resume_overlay(model, return_context);
        return None;
    }
    model
        .agent
        .ui_mut()
        .thinking_selector_mut()
        .open(return_context, options, active);
    None
}

pub(super) fn close(model: &mut Model) -> Option<Effect> {
    let return_context = model.agent.ui_mut().thinking_selector_mut().close();
    model.modal.close(Context::AgentThinkingSelector);
    super::dialog::resume_overlay(model, return_context);
    None
}

pub(super) fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => model.agent.ui_mut().thinking_selector_mut().select_next(),
        GeneralAction::Previous => model
            .agent
            .ui_mut()
            .thinking_selector_mut()
            .select_previous(),
        GeneralAction::Activate => return activate(model),
        GeneralAction::Back => return close(model),
        GeneralAction::FocusNext | GeneralAction::Paste => {}
    }
    None
}

fn activate(model: &mut Model) -> Option<Effect> {
    let id = model.agent.ui().thinking_selector().selected_id()?;
    model.agent.runtime_mut().set_thinking_id(id);
    close(model)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use eggplant_agent::{AgentProvider, AgentRequest, AgentStreamFuture};

    use super::*;

    fn options() -> Vec<ThinkingOption> {
        vec![
            ThinkingOption::new("off", "Off", "No reasoning"),
            ThinkingOption::new("medium2026", "Medium 2026", "Current reasoning"),
            ThinkingOption::new("reason-v1", "Reason V1", "Extended reasoning"),
        ]
    }

    #[derive(Debug)]
    struct OptionsProvider {
        options: Vec<ThinkingOption>,
    }

    impl AgentProvider for OptionsProvider {
        fn thinking_options(&self) -> &[ThinkingOption] {
            &self.options
        }

        fn stream<'a>(
            &'a self,
            _request: AgentRequest,
            _on_event: eggplant_agent::AgentEventSink<'a>,
        ) -> AgentStreamFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn list_selection_reuses_wrapping_navigation_for_dynamic_ids() {
        let mut state = ThinkingSelectorState::default();
        state.open(None, options(), Some(ThinkingId::new("medium2026")));
        assert_eq!(state.selected(), Some(1));
        state.select_next();
        assert_eq!(state.selected_id(), Some(ThinkingId::new("reason-v1")));
        state.select_previous();
        state.select_previous();
        assert_eq!(state.selected_id(), Some(ThinkingId::new("off")));
        state.select_previous();
        assert_eq!(state.selected_id(), Some(ThinkingId::new("reason-v1")));
    }

    #[test]
    fn activation_updates_runtime_and_closes_the_modal() {
        let mut model = Model::default();
        model
            .agent
            .set_provider(Arc::new(OptionsProvider { options: options() }));
        open(&mut model);
        assert!(model.modal.is_active(Context::AgentThinkingSelector));

        handle_general_action(&mut model, GeneralAction::Next);
        handle_general_action(&mut model, GeneralAction::Activate);

        assert_eq!(
            model.agent.runtime().thinking_id(),
            Some(ThinkingId::new("medium2026"))
        );
        assert!(!model.modal.is_active(Context::AgentThinkingSelector));
    }
}
