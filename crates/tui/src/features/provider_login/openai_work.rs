use std::sync::Arc;

use eggplant_agent::AgentProvider;
use eggplant_agent_storage::{ModelCatalog, ModelCatalogStore, ProviderCredentialStore};

use crate::{
    capabilities::openai::OpenAiProviderFactory,
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub enum OpenAiAfterSave {
    FetchModels(Effect),
    CloseWithoutCatalog,
}

pub fn after_save(model: &mut Model) -> OpenAiAfterSave {
    match (
        model.provider_credentials.clone(),
        model.openai_factory.clone(),
    ) {
        (Some(store), Some(factory)) => model
            .provider_login
            .begin_model_loading("openai")
            .map(|()| {
                OpenAiAfterSave::FetchModels(catalog_effect(
                    store,
                    factory,
                    model.model_catalogs.clone(),
                ))
            })
            .unwrap_or(OpenAiAfterSave::CloseWithoutCatalog),
        _ => OpenAiAfterSave::CloseWithoutCatalog,
    }
}

fn catalog_effect(
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    catalogs: Option<Arc<dyn ModelCatalogStore>>,
) -> Effect {
    Effect::feature(FetchModels {
        store,
        factory,
        catalogs,
    })
}

pub fn activate_selected(model: &mut Model) -> Option<Effect> {
    let store = model.provider_credentials.clone()?;
    let factory = model.openai_factory.clone()?;
    let model_id = model.provider_login.begin_activation("openai")?;
    Some(Effect::feature(ActivateModel {
        store,
        factory,
        model_id,
        updated_at: chrono::Utc::now().timestamp_millis(),
    }))
}

struct FetchModels {
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    catalogs: Option<Arc<dyn ModelCatalogStore>>,
}

impl FetchModels {
    async fn perform_async(self) -> ModelCatalogCompletion {
        let result = async {
            let credential = self
                .store
                .get("openai")
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "OpenAI credential was not found".to_string())?;
            self.factory.list_models(credential).await
        }
        .await;
        let cache_warning = match (&result, self.catalogs) {
            (Ok(models), Some(catalogs)) => {
                let checked_at = chrono::Utc::now().timestamp_millis();
                match ModelCatalog::new("openai", models.clone(), checked_at) {
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
        match model
            .provider_login
            .complete_model_loading("openai", self.result)
        {
            Some(Ok(())) => self.cache_warning.map(|warning| {
                crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!("OpenAI catalog cache warning: {warning}")),
                    Some("OpenAI"),
                )
            }),
            Some(Err(error)) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!("OpenAI model fetch failed: {error}")),
                Some("OpenAI"),
            )),
            None => None,
        }
    }
}

struct ActivateModel {
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
            self.model_id,
            self.updated_at,
        )
        .await;
        ModelActivationCompletion {
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
            .complete_activation("openai", error.clone());
        match (outcome, self.result) {
            (Some(true), Ok(provider)) => {
                model.agent.set_provider(provider);
                model
                    .agent
                    .set_provider_metadata("openai".to_string(), Some(self.model_id.clone()));
                super::work::close(model);
                Some(crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!("OpenAI ready · {}", self.model_id)),
                    Some("OpenAI"),
                ))
            }
            (Some(false), _) => Some(crate::notifications::publish_status(
                model,
                StatusMessage::error(format!(
                    "OpenAI activation failed: {}",
                    error.unwrap_or_else(|| "unknown error".to_string())
                )),
                Some("OpenAI"),
            )),
            _ => None,
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("OpenAI workflow runtime must build")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use eggplant_agent::{AgentEvent, AgentRequest};
    use eggplant_agent_storage_seaorm::SeaOrmProviderCredentialStore;

    use super::{super::work, *};
    use crate::{
        component::text_input::TextInputEvent, data_object::CommandId,
        manager::message_manager::MessageManager, provider_login::ProviderLoginPage,
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

    fn open_form(model: &mut Model) {
        work::open(model);
        work::handle_command(model, CommandId("provider_login_next"));
        work::handle_command(model, CommandId("provider_login_activate"));
        work::handle_command(model, CommandId("provider_login_activate"));
        work::handle_command(model, CommandId("provider_login_focus_next"));
        work::handle_input(model, TextInputEvent::Insert('k'));
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
