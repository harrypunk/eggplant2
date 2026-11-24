use std::collections::{HashMap, hash_map::Entry};

use color_eyre::eyre::eyre;

use crate::{
    data_object::{Context, GeneralAction, Model},
    manager::{effect::Effect, input_context::input_context},
};

/// Semantic action interface, independent of physical keys and feature command IDs.
pub trait GeneralActionHandler: std::fmt::Debug + Send + Sync {
    fn handle(&self, model: &mut Model, action: GeneralAction) -> Option<Effect>;
}

pub type GeneralActionFn = fn(&mut Model, GeneralAction) -> Option<Effect>;

/// A feature-provided subscription, registered at the composition root.
#[derive(Debug, Clone, Copy)]
pub struct GeneralActionSubscription {
    pub context: Context,
    pub handler: GeneralActionFn,
}

impl GeneralActionSubscription {
    pub const fn new(context: Context, handler: GeneralActionFn) -> Self {
        Self { context, handler }
    }
}

/// Feature-agnostic context-to-subscription dispatcher.
#[derive(Debug, Default)]
pub struct GeneralActionDispatcher {
    handlers: HashMap<Context, GeneralActionFn>,
}

impl GeneralActionDispatcher {
    pub fn new(
        subscriptions: impl IntoIterator<Item = GeneralActionSubscription>,
    ) -> color_eyre::Result<Self> {
        let mut dispatcher = Self::default();
        subscriptions
            .into_iter()
            .try_for_each(|subscription| dispatcher.register(subscription))?;
        Ok(dispatcher)
    }

    pub fn register(&mut self, subscription: GeneralActionSubscription) -> color_eyre::Result<()> {
        match self.handlers.entry(subscription.context) {
            Entry::Occupied(_) => Err(eyre!(
                "duplicate general action subscriber for `{:?}`",
                subscription.context
            )),
            Entry::Vacant(entry) => {
                entry.insert(subscription.handler);
                Ok(())
            }
        }
    }
}

impl GeneralActionHandler for GeneralActionDispatcher {
    fn handle(&self, model: &mut Model, action: GeneralAction) -> Option<Effect> {
        let context = input_context(model)?;
        let handler = self.handlers.get(&context)?;
        handler(model, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_login::{ProviderAuthMethod, ProviderLoginPage};

    #[test]
    fn provider_login_reuses_its_existing_workflow_for_general_actions() {
        let handler =
            GeneralActionDispatcher::new(crate::provider_login::general_action_subscriptions())
                .unwrap();
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

    #[test]
    fn duplicate_subscriptions_are_rejected() {
        let subscriptions = crate::provider_login::general_action_subscriptions();
        let mut dispatcher = GeneralActionDispatcher::new(subscriptions.clone()).unwrap();
        let error = dispatcher.register(subscriptions[0]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate general action subscriber")
        );
    }
}
