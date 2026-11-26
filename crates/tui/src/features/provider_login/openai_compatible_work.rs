use std::sync::Arc;

use eggplant_agent::AgentProvider;
use eggplant_agent_storage::{ModelCatalog, ModelCatalogStore, ProviderCredentialStore};

use crate::{
    capabilities::openai::OpenAiProviderFactory,
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub enum OpenAiCompatibleAfterSave {
    FetchModels(Effect),
    CloseWithoutCatalog,
}

pub fn after_save(model: &mut Model, provider_id: &str) -> OpenAiCompatibleAfterSave {
    match (
        model.provider_credentials.clone(),
        model.openai_factory.clone(),
    ) {
        (Some(store), Some(factory)) => model
            .provider_login
            .begin_model_loading(provider_id)
            .map(|()| {
                OpenAiCompatibleAfterSave::FetchModels(catalog_effect(
                    provider_id.to_string(),
                    store,
                    factory,
                    model.model_catalogs.clone(),
                ))
            })
            .unwrap_or(OpenAiCompatibleAfterSave::CloseWithoutCatalog),
        _ => OpenAiCompatibleAfterSave::CloseWithoutCatalog,
    }
}

fn catalog_effect(
    provider_id: String,
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    catalogs: Option<Arc<dyn ModelCatalogStore>>,
) -> Effect {
    Effect::feature(FetchModels {
        provider_id,
        store,
        factory,
        catalogs,
    })
}

pub fn activate_selected(model: &mut Model) -> Option<Effect> {
    let provider = match model.provider_login.page() {
        super::ProviderLoginPage::OpenAiModelSelection(provider) => provider,
        _ => return None,
    };
    let store = model.provider_credentials.clone()?;
    let factory = model.openai_factory.clone()?;
    let model_id = model.provider_login.begin_activation(provider.id())?;
    Some(Effect::feature(ActivateModel {
        provider_id: provider.id().to_string(),
        store,
        factory,
        model_id,
        updated_at: chrono::Utc::now().timestamp_millis(),
    }))
}

struct FetchModels {
    provider_id: String,
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    catalogs: Option<Arc<dyn ModelCatalogStore>>,
}

impl FetchModels {
    async fn perform_async(self) -> ModelCatalogCompletion {
        let Self {
            provider_id,
            store,
            factory,
            catalogs,
        } = self;
        let result = async {
            let credential = store
                .get(&provider_id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("{provider_id} credential was not found"))?;
            factory.list_models(credential).await
        }
        .await;
        let cache_warning = match (&result, catalogs) {
            (Ok(models), Some(catalogs)) => {
                let checked_at = chrono::Utc::now().timestamp_millis();
                match ModelCatalog::new(&provider_id, models.clone(), checked_at) {
                    Ok(catalog) => catalogs
                        .upsert(catalog)
                        .await
                        .err()
                        .map(|error| error.to_string()),
                    Err(error) => Some(error.to_string()),
                }
            }
            _ => None,
        };
        ModelCatalogCompletion {
            provider_id,
            result,
            cache_warning,
        }
    }
}

impl FeatureEffect for FetchModels {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct ModelCatalogCompletion {
    provider_id: String,
    result: Result<Vec<String>, String>,
    cache_warning: Option<String>,
}

impl ModelCatalogCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for ModelCatalogCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let Self {
            provider_id,
            result,
            cache_warning,
        } = *self;
        match model
            .provider_login
            .complete_model_loading(&provider_id, result)
        {
            Some(Ok(())) => cache_warning.map(|warning| {
                crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!("{provider_id} catalog cache warning: {warning}")),
                    Some(&provider_id),
                )
            }),
            Some(Err(error)) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!("{provider_id} model fetch failed: {error}")),
                Some(&provider_id),
            )),
            None => None,
        }
    }
}

struct ActivateModel {
    provider_id: String,
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    model_id: String,
    updated_at: i64,
}

impl ActivateModel {
    async fn perform_async(self) -> ModelActivationCompletion {
        let activation = crate::capabilities::openai::activate(
            self.store,
            self.factory,
            self.provider_id,
            self.model_id,
            self.updated_at,
        )
        .await;
        ModelActivationCompletion {
            provider_id: activation.provider_id,
            model_id: activation.model_id,
            result: activation.result,
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

struct ModelActivationCompletion {
    provider_id: String,
    model_id: String,
    result: Result<Arc<dyn AgentProvider>, String>,
}

impl ModelActivationCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for ModelActivationCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let error = self.result.as_ref().err().cloned();
        let outcome = model
            .provider_login
            .complete_activation(&self.provider_id, error.clone());
        match (outcome, self.result) {
            (Some(true), Ok(provider)) => {
                model.agent.set_provider(provider);
                model
                    .agent
                    .set_provider_metadata(self.provider_id.clone(), Some(self.model_id.clone()));
                super::work::close(model);
                Some(crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!("{} ready · {}", self.provider_id, self.model_id)),
                    Some(&self.provider_id),
                ))
            }
            (Some(false), _) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!(
                    "{} activation failed: {}",
                    self.provider_id,
                    error.unwrap_or_else(|| "unknown error".to_string())
                )),
                Some(&self.provider_id),
            )),
            _ => None,
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("OpenAI-compatible workflow runtime must build")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use eggplant_agent::{AgentEvent, AgentRequest};
    use eggplant_agent_storage_seaorm::SeaOrmProviderCredentialStore;

    use super::{super::work, *};
    use crate::{
        component::text_input::TextInputEvent,
        data_object::{CommandId, GeneralAction},
        manager::message_manager::MessageManager,
        provider_login::ProviderLoginPage,
    };

    #[derive(Debug)]
    struct FakeFactory {
        models: Result<Vec<String>, String>,
        activated: Arc<Mutex<Vec<String>>>,
    }

    impl OpenAiProviderFactory for FakeFactory {
        fn list_models(
            &self,
            _credential: eggplant_agent_storage::ProviderCredential,
        ) -> crate::capabilities::openai::OpenAiCatalogFuture<'_> {
            let models = self.models.clone();
            Box::pin(async move { models })
        }

        fn provider(
            &self,
            _credential: eggplant_agent_storage::ProviderCredential,
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

    fn model_with_factory(
        models: Result<Vec<String>, String>,
    ) -> (
        Model,
        Arc<SeaOrmProviderCredentialStore>,
        tempfile::TempDir,
        Arc<Mutex<Vec<String>>>,
    ) {
        let (store, directory) = crate::test_utils::provider_credential_store();
        let activated = Arc::new(Mutex::new(Vec::new()));
        let factory = FakeFactory {
            models,
            activated: activated.clone(),
        };
        let model = Model {
            provider_credentials: Some(store.clone()),
            openai_factory: Some(Arc::new(factory)),
            ..Model::default()
        };
        (model, store, directory, activated)
    }

    fn open_form_at(model: &mut Model, provider_index: usize) {
        work::open(model);
        work::handle_command(model, CommandId("provider_login_next"));
        work::handle_command(model, CommandId("provider_login_activate"));
        for _ in 0..provider_index {
            work::handle_command(model, CommandId("provider_login_next"));
        }
        work::handle_command(model, CommandId("provider_login_activate"));
        work::handle_general_action(model, GeneralAction::FocusNext);
        work::handle_input(model, TextInputEvent::Insert('k'));
    }

    fn open_form(model: &mut Model) {
        open_form_at(model, 0);
    }

    fn perform(model: &mut Model, effect: Effect) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected OpenAI feature effect");
        };
        MessageManager::default().update(model, request.perform())
    }

    #[test]
    fn login_fetches_selects_persists_and_activates_an_available_model() {
        let (mut model, store, _directory, activated) =
            model_with_factory(Ok(vec!["gpt-a".to_string(), "gpt-b".to_string()]));
        let (catalogs, _catalog_directory) = crate::test_utils::model_catalog_store();
        model.model_catalogs = Some(catalogs.clone());
        open_form(&mut model);
        let save = work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        let fetch = perform(&mut model, save).expect("successful save should fetch models");
        match model.provider_login.page() {
            ProviderLoginPage::OpenAiModelsLoading(_) => {}
            other => panic!("expected model loading, got {other:?}"),
        }
        perform(&mut model, fetch);
        match model.provider_login.page() {
            ProviderLoginPage::OpenAiModelSelection(_) => {}
            other => panic!("expected model selection, got {other:?}"),
        }
        assert_eq!(model.provider_login.models(), ["gpt-a", "gpt-b"]);

        work::handle_command(&mut model, CommandId("provider_login_next"));
        let activation =
            work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        perform(&mut model, activation);

        assert!(!model.provider_login.is_open());
        assert!(model.agent.provider().is_some());
        assert_eq!(model.agent.provider_name(), "openai");
        assert_eq!(model.agent.model(), Some("gpt-b"));
        assert_eq!(activated.lock().unwrap().as_slice(), ["gpt-b"]);
        let credential = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(store.get("openai"))
            .unwrap()
            .unwrap();
        assert_eq!(credential.selected_model.as_deref(), Some("gpt-b"));
        assert_eq!(
            credential.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        let catalog = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(catalogs.get("openai"))
            .unwrap()
            .unwrap();
        assert_eq!(catalog.models, ["gpt-a", "gpt-b"]);
        assert!(catalog.checked_at > 0);
    }

    #[test]
    fn kimi_login_has_an_independent_credential_catalog_and_active_model() {
        let (mut model, store, _directory, activated) = model_with_factory(Ok(vec![
            "kimi-for-coding".to_string(),
            "k3-256k".to_string(),
        ]));
        let (catalogs, _catalog_directory) = crate::test_utils::model_catalog_store();
        model.model_catalogs = Some(catalogs.clone());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime
            .block_on(store.upsert(eggplant_agent_storage::ProviderCredential {
                provider_id: "openai".to_string(),
                auth_type: eggplant_agent_storage::ProviderAuthType::ApiKey,
                base_url: Some("https://api.openai.com/v1".to_string()),
                selected_model: Some("gpt-existing".to_string()),
                api_key: Some("openai-secret".to_string()),
                access_token: None,
                refresh_token: None,
                expires_at: None,
                account_id: None,
                created_at: 1,
                updated_at: 1,
            }))
            .unwrap();

        open_form_at(&mut model, 1);
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::Endpoint)
                .value(),
            crate::provider_login::DEFAULT_KIMI_FOR_CODING_ENDPOINT
        );
        let save = work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        let fetch = perform(&mut model, save).expect("Kimi save should fetch its catalog");
        perform(&mut model, fetch);
        assert_eq!(
            model.provider_login.models(),
            ["kimi-for-coding", "k3-256k"]
        );

        work::handle_command(&mut model, CommandId("provider_login_next"));
        let activation =
            work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        perform(&mut model, activation);

        assert!(!model.provider_login.is_open());
        assert_eq!(model.agent.provider_name(), "kimi-for-coding");
        assert_eq!(model.agent.model(), Some("k3-256k"));
        assert_eq!(activated.lock().unwrap().as_slice(), ["k3-256k"]);
        let kimi = runtime
            .block_on(store.get("kimi-for-coding"))
            .unwrap()
            .unwrap();
        assert_eq!(kimi.api_key.as_deref(), Some("k"));
        assert_eq!(
            kimi.base_url.as_deref(),
            Some(crate::provider_login::DEFAULT_KIMI_FOR_CODING_ENDPOINT)
        );
        assert_eq!(kimi.selected_model.as_deref(), Some("k3-256k"));
        let openai = runtime.block_on(store.get("openai")).unwrap().unwrap();
        assert_eq!(openai.api_key.as_deref(), Some("openai-secret"));
        assert_eq!(openai.selected_model.as_deref(), Some("gpt-existing"));
        let catalog = runtime
            .block_on(catalogs.get("kimi-for-coding"))
            .unwrap()
            .unwrap();
        assert_eq!(catalog.models, ["k3-256k", "kimi-for-coding"]);
    }

    #[test]
    fn qwen_cn_login_uses_its_regional_endpoint_and_catalog() {
        let (mut model, store, _directory, activated) =
            model_with_factory(Ok(vec!["qwen3.7-max".to_string()]));
        let (catalogs, _catalog_directory) = crate::test_utils::model_catalog_store();
        model.model_catalogs = Some(catalogs.clone());
        open_form_at(&mut model, 2);
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::Endpoint)
                .value(),
            crate::provider_login::DEFAULT_QWEN_TOKENPLAN_INDIVIDUAL_CN_ENDPOINT
        );

        let save = work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        let fetch = perform(&mut model, save).expect("Qwen save should fetch its catalog");
        perform(&mut model, fetch);
        assert_eq!(model.provider_login.models(), ["qwen3.7-max"]);
        let activation =
            work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        perform(&mut model, activation);

        assert!(!model.provider_login.is_open());
        assert_eq!(model.agent.provider_name(), "qwen-tokenplan-individual-cn");
        assert_eq!(model.agent.model(), Some("qwen3.7-max"));
        assert_eq!(activated.lock().unwrap().as_slice(), ["qwen3.7-max"]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let credential = runtime
            .block_on(store.get("qwen-tokenplan-individual-cn"))
            .unwrap()
            .unwrap();
        assert_eq!(
            credential.base_url.as_deref(),
            Some(crate::provider_login::DEFAULT_QWEN_TOKENPLAN_INDIVIDUAL_CN_ENDPOINT)
        );
        assert_eq!(credential.selected_model.as_deref(), Some("qwen3.7-max"));
        assert!(
            runtime
                .block_on(catalogs.get("qwen-tokenplan-individual-cn"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn catalog_failure_preserves_the_form_and_does_not_activate() {
        let (mut model, _store, _directory, _activated) =
            model_with_factory(Err("catalog unavailable".to_string()));
        open_form(&mut model);
        let save = work::handle_command(&mut model, CommandId("provider_login_activate")).unwrap();
        let fetch = perform(&mut model, save).unwrap();
        perform(&mut model, fetch);

        match model.provider_login.page() {
            ProviderLoginPage::OpenAiInput(_) => {}
            other => panic!("expected preserved OpenAI form, got {other:?}"),
        }
        assert!(
            model
                .provider_login
                .error()
                .unwrap()
                .contains("catalog unavailable")
        );
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::ApiKey)
                .value(),
            "k"
        );
        assert!(model.agent.provider().is_none());
    }
}
