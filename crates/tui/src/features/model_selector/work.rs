use std::sync::Arc;

use eggplant_agent_storage::{
    ModelCatalog, ModelCatalogStore, ProviderCredential, ProviderCredentialStore,
};

use crate::{
    capabilities::openai::OpenAiProviderFactory,
    data_object::{CommandId, Context, GeneralAction, Message, Model, StatusMessage},
    manager::{
        command_registry::Command,
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        general_action::GeneralActionSubscription,
    },
    model_selector::ProviderModel,
};

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("model_selector_open"),
            "select provider model",
            handle_command,
        ),
        Command::feature(
            CommandId("model_selector_close"),
            "close model selector",
            handle_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![GeneralActionSubscription::new(
        Context::ModelSelector,
        handle_general_action,
    )]
}

pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => model.model_selector.select_next(),
        GeneralAction::Previous => model.model_selector.select_previous(),
        GeneralAction::Activate => return activate_selected(model),
        GeneralAction::Back => return close(model),
        GeneralAction::FocusNext | GeneralAction::Paste => {}
    }
    None
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "model_selector_open" => open(model),
        "model_selector_close" => close(model),
        _ => None,
    }
}

pub fn open(model: &mut Model) -> Option<Effect> {
    let Some(catalogs) = model.model_catalogs.clone() else {
        return Some(crate::notifications::publish_status(
            model,
            StatusMessage::info("Model storage is not configured".to_string()),
            Some("Provider"),
        ));
    };
    let active_model = model
        .agent
        .model()
        .map(|model_id| ProviderModel::new(model.agent.provider_name().to_string(), model_id));
    let return_context = crate::agent::suspend_for_overlay(model);
    if !model.modal.open(Context::ModelSelector) {
        crate::agent::resume_overlay(model, return_context);
        return None;
    }
    model.model_selector.open(return_context, active_model);
    let generation = model.model_selector.generation();
    Some(Effect::feature(LoadCatalogs {
        store: model.provider_credentials.clone(),
        catalogs,
        generation,
    }))
}

pub fn close(model: &mut Model) -> Option<Effect> {
    let return_context = model.model_selector.close();
    model.modal.close(Context::ModelSelector);
    crate::agent::resume_overlay(model, return_context);
    None
}

struct ProviderCatalog {
    credential: ProviderCredential,
    cached: Option<ModelCatalog>,
}

struct LoadCatalogs {
    store: Option<Arc<dyn ProviderCredentialStore>>,
    catalogs: Arc<dyn ModelCatalogStore>,
    generation: u64,
}

impl LoadCatalogs {
    async fn perform_async(self) -> LoadCatalogsCompletion {
        let mut providers = Vec::new();
        if let Some(store) = self.store {
            let infos = store.list().await.unwrap_or_default();
            for info in infos {
                let credential = match store.get(&info.provider_id).await {
                    Ok(Some(credential)) if credential.base_url.is_some() => credential,
                    _ => continue,
                };
                let cached = self
                    .catalogs
                    .get(&credential.provider_id)
                    .await
                    .ok()
                    .flatten();
                providers.push(ProviderCatalog { credential, cached });
            }
        }
        LoadCatalogsCompletion {
            providers,
            generation: self.generation,
        }
    }
}

impl FeatureEffect for LoadCatalogs {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct LoadCatalogsCompletion {
    providers: Vec<ProviderCatalog>,
    generation: u64,
}

impl LoadCatalogsCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for LoadCatalogsCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if !model.model_selector.is_current(self.generation) {
            return None;
        }
        let cached = self
            .providers
            .iter()
            .filter_map(|provider| provider.cached.clone())
            .collect();
        model.model_selector.apply_cached(cached);
        refresh(model, self.providers)
    }
}

fn refresh(model: &mut Model, providers: Vec<ProviderCatalog>) -> Option<Effect> {
    if providers.is_empty() {
        model
            .model_selector
            .fail_refresh("No OpenAI-compatible credentials; use /provider first".to_string());
        return None;
    }
    let Some(factory) = model.openai_factory.clone() else {
        model
            .model_selector
            .fail_refresh("Model provider factory is not configured".to_string());
        return None;
    };
    let catalogs = model.model_catalogs.clone()?;
    let generation = model.model_selector.generation();
    Some(Effect::feature(RefreshCatalogs {
        catalogs,
        factory,
        providers,
        generation,
    }))
}

struct RefreshCatalogs {
    catalogs: Arc<dyn ModelCatalogStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    providers: Vec<ProviderCatalog>,
    generation: u64,
}

impl RefreshCatalogs {
    async fn perform_async(self) -> RefreshCatalogsCompletion {
        let mut tasks = tokio::task::JoinSet::new();
        for provider in self.providers {
            let factory = self.factory.clone();
            let catalogs = self.catalogs.clone();
            tasks.spawn(refresh_catalog(provider, factory, catalogs));
        }
        let mut catalogs = Vec::new();
        let mut errors = Vec::new();
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(catalog)) => catalogs.push(catalog),
                Ok(Err(error)) => errors.push(error),
                Err(error) => errors.push(format!("catalog refresh task failed: {error}")),
            }
        }
        RefreshCatalogsCompletion {
            catalogs,
            errors,
            generation: self.generation,
        }
    }
}

async fn refresh_catalog(
    provider: ProviderCatalog,
    factory: Arc<dyn OpenAiProviderFactory>,
    catalogs: Arc<dyn ModelCatalogStore>,
) -> Result<ModelCatalog, String> {
    let provider_id = provider.credential.provider_id.clone();
    async {
        let models = factory.list_models(provider.credential).await?;
        let checked_at = chrono::Utc::now().timestamp_millis();
        let catalog = ModelCatalog::new(&provider_id, models, checked_at)
            .map_err(|error| error.to_string())?;
        catalogs
            .upsert(catalog.clone())
            .await
            .map_err(|error| error.to_string())?;
        Ok(catalog)
    }
    .await
    .map_err(|error: String| format!("{provider_id}: {error}"))
}

impl FeatureEffect for RefreshCatalogs {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct RefreshCatalogsCompletion {
    catalogs: Vec<ModelCatalog>,
    errors: Vec<String>,
    generation: u64,
}

impl RefreshCatalogsCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for RefreshCatalogsCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if !model.model_selector.is_current(self.generation) {
            return None;
        }
        model
            .model_selector
            .apply_refresh(self.catalogs, self.errors);
        None
    }
}

fn activate_selected(model: &mut Model) -> Option<Effect> {
    let store = model.provider_credentials.clone()?;
    let factory = model.openai_factory.clone()?;
    let (selected, generation) = model.model_selector.begin_activation()?;
    Some(Effect::feature(ActivateModel {
        store,
        factory,
        provider_id: selected.provider_id().to_string(),
        model_id: selected.model_id().to_string(),
        generation,
        updated_at: chrono::Utc::now().timestamp_millis(),
    }))
}

struct ActivateModel {
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    provider_id: String,
    model_id: String,
    generation: u64,
    updated_at: i64,
}

impl ActivateModel {
    async fn perform_async(self) -> ActivateModelCompletion {
        let generation = self.generation;
        let activation = crate::capabilities::openai::activate(
            self.store,
            self.factory,
            self.provider_id,
            self.model_id,
            self.updated_at,
        )
        .await;
        ActivateModelCompletion {
            generation,
            activation,
        }
    }
}

impl FeatureEffect for ActivateModel {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct ActivateModelCompletion {
    generation: u64,
    activation: crate::capabilities::openai::OpenAiActivationResult,
}

impl ActivateModelCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for ActivateModelCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let error = self.activation.result.as_ref().err().cloned();
        let outcome = model
            .model_selector
            .complete_activation(self.generation, error.clone());
        match (outcome, self.activation.result) {
            (Some(true), Ok(provider)) => {
                model.agent.set_provider(provider);
                model.agent.set_provider_metadata(
                    self.activation.provider_id.clone(),
                    Some(self.activation.model_id.clone()),
                );
                close(model);
                Some(crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!(
                        "Model: {}/{}",
                        self.activation.provider_id, self.activation.model_id
                    )),
                    Some(&self.activation.provider_id),
                ))
            }
            (Some(false), _) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!(
                    "Model activation failed: {}",
                    error.unwrap_or_else(|| "unknown error".to_string())
                )),
                Some("Provider"),
            )),
            _ => None,
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("model selector runtime must build")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use eggplant_agent::{AgentEvent, AgentProvider, AgentRequest};
    use eggplant_agent_storage::{ProviderAuthType, ProviderCredential};
    use eggplant_agent_storage_seaorm::{SeaOrmModelCatalogStore, SeaOrmProviderCredentialStore};

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
        model_selector::{ModelSelectorNotice, ModelSelectorPage},
    };

    #[derive(Debug)]
    struct FakeFactory {
        models: Result<Vec<String>, String>,
        activated: Arc<Mutex<Vec<String>>>,
    }

    impl OpenAiProviderFactory for FakeFactory {
        fn list_models(
            &self,
            _credential: ProviderCredential,
        ) -> crate::capabilities::openai::OpenAiCatalogFuture<'_> {
            let models = self.models.clone();
            Box::pin(async move { models })
        }

        fn provider(
            &self,
            _credential: ProviderCredential,
            model: String,
        ) -> Result<Arc<dyn AgentProvider>, String> {
            self.activated.lock().unwrap().push(model);
            Ok(Arc::new(FakeProvider))
        }
    }

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

    struct TestContext {
        model: Model,
        store: Arc<SeaOrmProviderCredentialStore>,
        catalogs: Arc<SeaOrmModelCatalogStore>,
        _directory: tempfile::TempDir,
        activated: Arc<Mutex<Vec<String>>>,
    }

    fn model_with(models: Result<Vec<String>, String>, cached: Option<Vec<String>>) -> TestContext {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.sqlite");
        let store = Arc::new(
            runtime()
                .block_on(SeaOrmProviderCredentialStore::connect(&path))
                .unwrap(),
        );
        let catalogs = Arc::new(
            runtime()
                .block_on(SeaOrmModelCatalogStore::connect(&path))
                .unwrap(),
        );
        let activated = Arc::new(Mutex::new(Vec::new()));
        let factory = FakeFactory {
            models,
            activated: activated.clone(),
        };
        if let Some(models) = cached {
            let catalog = ModelCatalog::new("openai", models, 1).unwrap();
            runtime().block_on(catalogs.upsert(catalog)).unwrap();
        }
        let credential = ProviderCredential {
            provider_id: "openai".to_string(),
            auth_type: ProviderAuthType::ApiKey,
            base_url: Some("https://api.openai.com/v1".to_string()),
            selected_model: None,
            api_key: Some("key-secret".to_string()),
            access_token: None,
            refresh_token: None,
            expires_at: None,
            account_id: None,
            created_at: 1,
            updated_at: 1,
        };
        runtime().block_on(store.upsert(credential)).unwrap();
        let mut model = Model {
            provider_credentials: Some(store.clone()),
            model_catalogs: Some(catalogs.clone()),
            openai_factory: Some(Arc::new(factory)),
            ..Model::default()
        };
        model
            .agent
            .set_provider_metadata("openai".to_string(), None);
        TestContext {
            model,
            store,
            catalogs,
            _directory: directory,
            activated,
        }
    }

    fn perform(model: &mut Model, effect: Effect) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected model selector feature effect");
        };
        crate::manager::message_manager::MessageManager::default().update(model, request.perform())
    }

    fn model_ids(models: &[ProviderModel]) -> Vec<&str> {
        models.iter().map(ProviderModel::model_id).collect()
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
    ) -> Option<Effect> {
        let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
        match events.translate_event(event, model).unwrap() {
            TranslatedEvent::Input(input) => messages.update_input(model, input),
            TranslatedEvent::System(message) => messages.update(model, message),
        }
    }

    async fn complete_async(messages: &MessageManager, model: &mut Model, effect: Effect) {
        let Effect::Feature(work) = effect else {
            panic!("expected asynchronous model activation");
        };
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        work.execute(sender);
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
            .await
            .expect("model activation should complete without stalling")
            .expect("activation channel should remain open");
        messages.update(model, message);
    }

    #[test]
    fn cached_catalog_refreshes_persists_and_switches_the_active_model() {
        let TestContext {
            mut model,
            store,
            catalogs,
            _directory,
            activated,
        } = model_with(
            Ok(vec!["gpt-a".to_string(), "gpt-b".to_string()]),
            Some(vec!["gpt-old".to_string(), "gpt-b".to_string()]),
        );
        let load = open(&mut model).unwrap();
        let refresh = perform(&mut model, load).unwrap();
        assert_eq!(
            model_ids(model.model_selector.models()),
            ["gpt-b", "gpt-old"]
        );

        perform(&mut model, refresh);
        assert_eq!(model_ids(model.model_selector.models()), ["gpt-a", "gpt-b"]);
        let refreshed = runtime().block_on(catalogs.get("openai")).unwrap().unwrap();
        assert_eq!(refreshed.models, ["gpt-a", "gpt-b"]);
        assert_eq!(
            model
                .model_selector
                .selected_model()
                .map(ProviderModel::model_id),
            Some("gpt-b")
        );

        let activation = activate_selected(&mut model).unwrap();
        perform(&mut model, activation);

        assert_eq!(model.model_selector.page(), ModelSelectorPage::Closed);
        assert!(model.agent.provider().is_some());
        assert_eq!(model.agent.provider_name(), "openai");
        assert_eq!(model.agent.model(), Some("gpt-b"));
        assert_eq!(activated.lock().unwrap().as_slice(), ["gpt-b"]);
        let credential = runtime().block_on(store.get("openai")).unwrap().unwrap();
        assert_eq!(credential.selected_model.as_deref(), Some("gpt-b"));
    }

    #[test]
    fn real_key_dispatch_navigates_and_completes_activation_without_stalling() {
        let TestContext {
            mut model,
            store: _,
            catalogs: _,
            _directory,
            activated,
        } = model_with(
            Ok(vec!["gpt-a".to_string(), "gpt-b".to_string()]),
            Some(vec!["gpt-a".to_string(), "gpt-b".to_string()]),
        );
        let load = open(&mut model).unwrap();
        let refresh = perform(&mut model, load).unwrap();
        perform(&mut model, refresh);
        let events = event_manager();
        let messages = MessageManager::default();
        assert_eq!(
            model
                .model_selector
                .selected_model()
                .map(ProviderModel::model_id),
            Some("gpt-a")
        );

        send_key(&events, &messages, &mut model, KeyCode::Down);
        assert_eq!(
            model
                .model_selector
                .selected_model()
                .map(ProviderModel::model_id),
            Some("gpt-b")
        );

        let activation = send_key(&events, &messages, &mut model, KeyCode::Enter)
            .expect("Enter should start model activation");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(complete_async(&messages, &mut model, activation));

        assert_eq!(model.model_selector.page(), ModelSelectorPage::Closed);
        assert_eq!(model.agent.model(), Some("gpt-b"));
        assert_eq!(activated.lock().unwrap().as_slice(), ["gpt-b"]);
    }

    #[test]
    fn model_selection_uses_the_active_kimi_credential_and_catalog() {
        let TestContext {
            mut model,
            store,
            catalogs,
            _directory,
            activated,
        } = model_with(
            Ok(vec![
                "k3-256k".to_string(),
                "kimi-for-coding-highspeed".to_string(),
            ]),
            None,
        );
        runtime()
            .block_on(store.upsert(ProviderCredential {
                provider_id: "kimi-for-coding".to_string(),
                auth_type: ProviderAuthType::ApiKey,
                base_url: Some(crate::provider_login::DEFAULT_KIMI_FOR_CODING_ENDPOINT.to_string()),
                selected_model: None,
                api_key: Some("kimi-secret".to_string()),
                access_token: None,
                refresh_token: None,
                expires_at: None,
                account_id: None,
                created_at: 1,
                updated_at: 1,
            }))
            .unwrap();
        model
            .agent
            .set_provider_metadata("kimi-for-coding".to_string(), None);

        let load = open(&mut model).unwrap();
        let refresh = perform(&mut model, load).unwrap();
        perform(&mut model, refresh);
        assert_eq!(
            model_ids(model.model_selector.models()),
            [
                "k3-256k",
                "kimi-for-coding-highspeed",
                "k3-256k",
                "kimi-for-coding-highspeed"
            ]
        );
        let providers = model
            .model_selector
            .models()
            .iter()
            .map(ProviderModel::provider_id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            providers,
            std::collections::BTreeSet::from(["kimi-for-coding", "openai"])
        );

        let activation = activate_selected(&mut model).unwrap();
        perform(&mut model, activation);

        assert_eq!(model.agent.provider_name(), "kimi-for-coding");
        assert_eq!(model.agent.model(), Some("k3-256k"));
        assert_eq!(activated.lock().unwrap().as_slice(), ["k3-256k"]);
        let kimi = runtime()
            .block_on(store.get("kimi-for-coding"))
            .unwrap()
            .unwrap();
        assert_eq!(kimi.selected_model.as_deref(), Some("k3-256k"));
        let openai = runtime().block_on(store.get("openai")).unwrap().unwrap();
        assert_eq!(openai.selected_model, None);
        let openai_catalog = runtime().block_on(catalogs.get("openai")).unwrap().unwrap();
        assert_eq!(
            openai_catalog.models,
            ["k3-256k", "kimi-for-coding-highspeed"]
        );
        assert!(
            runtime()
                .block_on(catalogs.get("kimi-for-coding"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn refresh_failure_preserves_cached_models() {
        let TestContext {
            mut model,
            store: _,
            catalogs: _,
            _directory,
            activated: _,
        } = model_with(
            Err("offline".to_string()),
            Some(vec!["gpt-old".to_string()]),
        );
        let load = open(&mut model).unwrap();
        let refresh = perform(&mut model, load).unwrap();
        perform(&mut model, refresh);

        assert_eq!(model_ids(model.model_selector.models()), ["gpt-old"]);
        assert_eq!(
            model.model_selector.notice(),
            Some(&ModelSelectorNotice::Warning("openai: offline".to_string()))
        );
        assert!(model.agent.provider().is_none());
    }

    #[test]
    fn late_refresh_is_ignored_after_close() {
        let TestContext {
            mut model,
            store: _,
            catalogs: _,
            _directory,
            activated: _,
        } = model_with(
            Ok(vec!["gpt-a".to_string()]),
            Some(vec!["gpt-old".to_string()]),
        );
        let load = open(&mut model).unwrap();
        let refresh = perform(&mut model, load).unwrap();
        close(&mut model);
        perform(&mut model, refresh);

        assert_eq!(model.model_selector.page(), ModelSelectorPage::Closed);
        assert!(model.model_selector.models().is_empty());
    }
}
