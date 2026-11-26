use std::sync::Arc;

use eggplant_agent_storage::{ProviderCredentialInfo, ProviderCredentialStore, StorageError};

use crate::{
    data_object::{CommandId, Context, GeneralAction, Message, Model, StatusMessage},
    manager::{
        command_registry::Command,
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        general_action::GeneralActionSubscription,
    },
};

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("provider_logout_open"),
            "log out of a provider",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_logout_close"),
            "close provider logout",
            handle_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![GeneralActionSubscription::new(
        Context::ProviderLogout,
        handle_general_action,
    )]
}

pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => model.provider_logout.select_next(),
        GeneralAction::Previous => model.provider_logout.select_previous(),
        GeneralAction::Activate => return delete_selected(model),
        GeneralAction::Back => return close(model),
        GeneralAction::FocusNext | GeneralAction::Paste => {}
    }
    None
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "provider_logout_open" => open(model),
        "provider_logout_close" => close(model),
        _ => None,
    }
}

pub fn open(model: &mut Model) -> Option<Effect> {
    let Some(store) = model.provider_credentials.clone() else {
        return Some(crate::notifications::publish_status(
            model,
            StatusMessage::info("Provider storage is not configured".to_string()),
            Some("Provider"),
        ));
    };
    let return_context = crate::agent::suspend_for_overlay(model);
    if !model.modal.open(Context::ProviderLogout) {
        crate::agent::resume_overlay(model, return_context);
        return None;
    }
    model.provider_logout.open(return_context);
    let generation = model.provider_logout.generation();
    Some(Effect::feature(LoadProviders { store, generation }))
}

pub fn close(model: &mut Model) -> Option<Effect> {
    let return_context = model.provider_logout.close();
    model.modal.close(Context::ProviderLogout);
    crate::agent::resume_overlay(model, return_context);
    None
}

fn delete_selected(model: &mut Model) -> Option<Effect> {
    let store = model.provider_credentials.clone()?;
    let (provider_id, generation) = model.provider_logout.begin_delete()?;
    Some(Effect::feature(DeleteCredential {
        store,
        provider_id,
        generation,
    }))
}

struct LoadProviders {
    store: Arc<dyn ProviderCredentialStore>,
    generation: u64,
}

impl LoadProviders {
    async fn perform_async(self) -> LoadProvidersCompletion {
        LoadProvidersCompletion {
            result: self.store.list().await,
            generation: self.generation,
        }
    }
}

impl FeatureEffect for LoadProviders {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct LoadProvidersCompletion {
    result: Result<Vec<ProviderCredentialInfo>, StorageError>,
    generation: u64,
}

impl LoadProvidersCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for LoadProvidersCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let error = self.result.as_ref().err().map(ToString::to_string);
        let result = self.result.map_err(|error| error.to_string());
        match model.provider_logout.complete_load(self.generation, result) {
            Some(super::ProviderLogoutLoad::Ready) => None,
            Some(super::ProviderLogoutLoad::Empty) => {
                close(model);
                Some(crate::notifications::publish_status(
                    model,
                    StatusMessage::info("No stored credentials to remove".to_string()),
                    Some("Provider"),
                ))
            }
            Some(super::ProviderLogoutLoad::Failed) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!(
                    "Could not read stored credentials: {}",
                    error.unwrap_or_else(|| "unknown storage error".to_string())
                )),
                Some("Provider"),
            )),
            None => None,
        }
    }
}

struct DeleteCredential {
    store: Arc<dyn ProviderCredentialStore>,
    provider_id: String,
    generation: u64,
}

impl DeleteCredential {
    async fn perform_async(self) -> DeleteCredentialCompletion {
        let result = self
            .store
            .delete(&self.provider_id)
            .await
            .map_err(|error| error.to_string());
        DeleteCredentialCompletion {
            provider_id: self.provider_id,
            generation: self.generation,
            result,
        }
    }
}

impl FeatureEffect for DeleteCredential {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct DeleteCredentialCompletion {
    provider_id: String,
    generation: u64,
    result: Result<bool, String>,
}

impl DeleteCredentialCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for DeleteCredentialCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let error = self.result.err();
        match model
            .provider_logout
            .complete_delete(self.generation, error.clone())
        {
            Some(true) => {
                if model.agent.provider_name() == self.provider_id {
                    model.agent.clear_provider();
                }
                close(model);
                Some(crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!("Logged out of {}", self.provider_id)),
                    Some("Provider"),
                ))
            }
            Some(false) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!(
                    "Logout failed: {}",
                    error.unwrap_or_else(|| "unknown storage error".to_string())
                )),
                Some("Provider"),
            )),
            None => None,
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("provider logout runtime must build")
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use eggplant_agent::{AgentEvent, AgentProvider, AgentRequest};
    use eggplant_agent_storage::{ProviderAuthType, ProviderCredential};
    use eggplant_agent_storage_seaorm::SeaOrmProviderCredentialStore;

    use super::*;
    use crate::{
        config::Config,
        manager::{
            command_registry::CommandRegistry,
            event_manager::{EventManager, TranslatedEvent},
            keyboard_input_registry::Registry,
            message_manager::MessageManager,
            text_input_registry::TextInputRegistry,
        },
    };

    #[derive(Debug)]
    struct FakeProvider;

    impl AgentProvider for FakeProvider {
        fn stream<'a>(
            &'a self,
            _request: AgentRequest,
            _on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<(), eggplant_agent::AgentError>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    fn credential(provider_id: &str) -> ProviderCredential {
        ProviderCredential {
            provider_id: provider_id.to_string(),
            auth_type: ProviderAuthType::ApiKey,
            base_url: Some("https://example.test/v1".to_string()),
            selected_model: None,
            api_key: Some("secret".to_string()),
            access_token: None,
            refresh_token: None,
            expires_at: None,
            account_id: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn model_with_providers() -> (Model, Arc<SeaOrmProviderCredentialStore>, tempfile::TempDir) {
        let (store, directory) = crate::test_utils::provider_credential_store();
        for provider_id in ["openai", "qwen-tokenplan-individual-cn"] {
            runtime()
                .block_on(store.upsert(credential(provider_id)))
                .unwrap();
        }
        let model = Model {
            provider_credentials: Some(store.clone()),
            ..Model::default()
        };
        (model, store, directory)
    }

    fn perform(model: &mut Model, effect: Effect) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected provider logout feature effect");
        };
        MessageManager::default().update(model, request.perform())
    }

    fn event_manager() -> EventManager {
        let config = Config::default();
        let keymap = Registry::from_config(
            &config.keymap,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();
        EventManager::new(&config, keymap, TextInputRegistry::builtin()).unwrap()
    }

    fn send_key(
        events: &EventManager,
        messages: &MessageManager,
        model: &mut Model,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<Effect> {
        let event = Event::Key(KeyEvent::new(key, modifiers));
        match events.translate_event(event, model).unwrap() {
            TranslatedEvent::Input(input) => messages.update_input(model, input),
            TranslatedEvent::System(message) => messages.update(model, message),
        }
    }

    #[test]
    fn navigation_deletes_selected_provider_and_clears_active_runtime() {
        let (mut model, store, _directory) = model_with_providers();
        model.agent.set_provider(Arc::new(FakeProvider));
        model.agent.set_provider_metadata(
            "qwen-tokenplan-individual-cn".to_string(),
            Some("qwen-a".to_string()),
        );
        let load = open(&mut model).unwrap();
        perform(&mut model, load);
        let events = event_manager();
        let messages = MessageManager::default();

        send_key(
            &events,
            &messages,
            &mut model,
            KeyCode::Down,
            KeyModifiers::NONE,
        );
        assert_eq!(
            model.provider_logout.selected_provider_id(),
            Some("qwen-tokenplan-individual-cn")
        );
        send_key(
            &events,
            &messages,
            &mut model,
            KeyCode::Char('p'),
            KeyModifiers::CONTROL,
        );
        assert_eq!(model.provider_logout.selected_provider_id(), Some("openai"));
        send_key(
            &events,
            &messages,
            &mut model,
            KeyCode::Char('n'),
            KeyModifiers::CONTROL,
        );
        assert_eq!(
            model.provider_logout.selected_provider_id(),
            Some("qwen-tokenplan-individual-cn")
        );

        let deletion = send_key(
            &events,
            &messages,
            &mut model,
            KeyCode::Enter,
            KeyModifiers::NONE,
        )
        .unwrap();
        perform(&mut model, deletion);

        let runtime = runtime();
        assert!(runtime.block_on(store.get("openai")).unwrap().is_some());
        assert!(
            runtime
                .block_on(store.get("qwen-tokenplan-individual-cn"))
                .unwrap()
                .is_none()
        );
        assert!(model.agent.provider().is_none());
        assert_eq!(model.agent.provider_name(), "fake");
        assert_eq!(model.agent.model(), None);
        assert!(!model.modal.is_active(Context::ProviderLogout));
    }

    #[test]
    fn no_stored_credentials_closes_with_a_notice() {
        let (store, _directory) = crate::test_utils::provider_credential_store();
        let mut model = Model {
            provider_credentials: Some(store),
            ..Model::default()
        };

        let load = open(&mut model).unwrap();
        perform(&mut model, load);

        assert!(!model.modal.is_active(Context::ProviderLogout));
        assert!(
            model
                .notifications
                .current()
                .unwrap()
                .message()
                .contains("No stored credentials")
        );
    }
}
