pub mod example_feature;

pub use example_feature::ExampleFeatureState;

use std::{path::Path, sync::Arc};

use editor::{BufferId, DocumentFactory};
use eggplant_agent_storage_seaorm::{SeaOrmProviderCredentialStore, SeaOrmSessionStore};

use crate::data_object::Model;

pub fn create_buffer(model: &mut Model, text: &str) -> BufferId {
    let document = DocumentFactory::new().create(text);
    model.editor.insert_document(document)
}

pub fn open_file(model: &mut Model, path: &Path) -> BufferId {
    let document = DocumentFactory::new().open(path).unwrap();
    model.editor.insert_document(document)
}

pub fn session_store() -> (Arc<SeaOrmSessionStore>, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let store = runtime()
        .block_on(SeaOrmSessionStore::connect(
            directory.path().join("agent.sqlite"),
        ))
        .unwrap();
    (Arc::new(store), directory)
}

pub fn provider_credential_store() -> (Arc<SeaOrmProviderCredentialStore>, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let store = runtime()
        .block_on(SeaOrmProviderCredentialStore::connect(
            directory.path().join("agent.sqlite"),
        ))
        .unwrap();
    (Arc::new(store), directory)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
