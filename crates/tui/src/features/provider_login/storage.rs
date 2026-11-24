use std::sync::Arc;

use eggplant_agent_storage::{ProviderCredential, ProviderCredentialStore, StorageError};

use crate::{
    data_object::{Message, Model},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub fn persist(store: Arc<dyn ProviderCredentialStore>, credential: ProviderCredential) -> Effect {
    Effect::feature(SaveApiKeyEffect { store, credential })
}

struct SaveApiKeyEffect {
    store: Arc<dyn ProviderCredentialStore>,
    credential: ProviderCredential,
}

impl SaveApiKeyEffect {
    async fn perform_async(self) -> SaveApiKeyCompletion {
        let provider_id = self.credential.provider_id.clone();
        let result = self.store.upsert(self.credential).await;
        SaveApiKeyCompletion {
            provider_id,
            result,
        }
    }
}

impl FeatureEffect for SaveApiKeyEffect {
    fn perform(self: Box<Self>) -> Message {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("provider credential storage runtime must build")
            .block_on(self.perform_async())
            .into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct SaveApiKeyCompletion {
    provider_id: String,
    result: Result<(), StorageError>,
}

impl SaveApiKeyCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for SaveApiKeyCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        super::work::complete_save(model, &self.provider_id, self.result)
    }
}
