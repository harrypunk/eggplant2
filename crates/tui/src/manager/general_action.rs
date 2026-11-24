use crate::{
    data_object::{Context, GeneralAction, Model},
    manager::{effect::Effect, input_context::input_context},
};

/// Semantic action interface, independent of physical keys and feature command IDs.
pub trait GeneralActionHandler: std::fmt::Debug + Send + Sync {
    fn handle(&self, model: &mut Model, action: GeneralAction) -> Option<Effect>;
}

#[derive(Debug, Default)]
pub struct BuiltinGeneralActionHandler;

impl GeneralActionHandler for BuiltinGeneralActionHandler {
    fn handle(&self, model: &mut Model, action: GeneralAction) -> Option<Effect> {
        match input_context(model)? {
            Context::ProviderLogin => crate::provider_login::handle_general_action(model, action),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_login::{ProviderAuthMethod, ProviderLoginPage};

    #[test]
    fn provider_login_reuses_its_existing_workflow_for_general_actions() {
        let handler = BuiltinGeneralActionHandler;
        let mut model = Model::default();
        crate::provider_login::open(&mut model);

        handler.handle(&mut model, GeneralAction::Next);
        assert_eq!(model.provider_login.selected(), Some(1));
        handler.handle(&mut model, GeneralAction::Activate);
        assert_eq!(
            model.provider_login.page(),
            ProviderLoginPage::Providers(ProviderAuthMethod::ApiKey)
        );
        handler.handle(&mut model, GeneralAction::Back);
        assert_eq!(
            model.provider_login.page(),
            ProviderLoginPage::AuthenticationMethod
        );
        handler.handle(&mut model, GeneralAction::Back);
        assert!(!model.provider_login.is_open());
    }
}
