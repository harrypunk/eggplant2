use crate::manager::general_action::{GeneralActionDispatcher, GeneralActionSubscription};

type SubscriptionProvider = fn() -> Vec<GeneralActionSubscription>;

const PROVIDERS: &[SubscriptionProvider] = &[
    crate::file_picker::general_action_subscriptions,
    crate::theme_picker::general_action_subscriptions,
    crate::buffer_grep::general_action_subscriptions,
    crate::project_grep::general_action_subscriptions,
    crate::agent::general_action_subscriptions,
    crate::model_selector::general_action_subscriptions,
    crate::provider_login::general_action_subscriptions,
];

/// Assemble feature-owned general-action subscriptions at the composition root.
pub fn general_actions() -> GeneralActionDispatcher {
    let subscriptions = PROVIDERS.iter().flat_map(|provider| provider());
    GeneralActionDispatcher::new(subscriptions)
        .expect("general action subscriptions must be unique")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_subscriptions_are_unique() {
        let _dispatcher = general_actions();
    }
}
