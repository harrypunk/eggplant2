use crate::{
    agent::AgentUiAction,
    data_object::{Model, StatusMessage},
    manager::effect::Effect,
};

pub(super) fn new_session(model: &mut Model) -> Option<Effect> {
    match model.agent.runtime_mut().new_session() {
        Some(_) => {
            model.agent.dispatch(AgentUiAction::SessionChanged);
            None
        }
        None => Some(crate::notifications::publish_status(
            model,
            StatusMessage::info(
                "Finish or interrupt the active run before starting a new session".to_string(),
            ),
            Some("Agent"),
        )),
    }
}
