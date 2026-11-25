use std::collections::BTreeSet;

use crate::{StorageError, StoreFuture};

pub const MAX_CATALOG_MODELS: usize = 256;
pub const MAX_CATALOG_MODEL_ID_BYTES: usize = 256;

/// Provider-scoped model catalog with no authentication material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalog {
    pub provider_id: String,
    pub models: Vec<String>,
    pub checked_at: i64,
    pub last_modified: Option<i64>,
    pub etag: Option<String>,
}

impl ModelCatalog {
    pub fn new(
        provider_id: impl Into<String>,
        models: Vec<String>,
        checked_at: i64,
    ) -> Result<Self, StorageError> {
        let provider_id = provider_id.into();
        if provider_id.trim().is_empty() {
            return Err(StorageError::Backend(
                "model catalog provider ID must not be empty".to_string(),
            ));
        }
        let models = models
            .into_iter()
            .filter_map(|model| {
                let model = model.trim().to_string();
                (!model.is_empty() && model.len() <= MAX_CATALOG_MODEL_ID_BYTES).then_some(model)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .take(MAX_CATALOG_MODELS)
            .collect();
        Ok(Self {
            provider_id,
            models,
            checked_at,
            last_modified: None,
            etag: None,
        })
    }

    pub fn with_validators(mut self, last_modified: Option<i64>, etag: Option<String>) -> Self {
        self.last_modified = last_modified;
        self.etag = etag;
        self
    }
}

/// Backend-neutral persistent model catalog boundary.
pub trait ModelCatalogStore: std::fmt::Debug + Send + Sync {
    fn get<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, Option<ModelCatalog>>;

    fn upsert<'a>(&'a self, catalog: ModelCatalog) -> StoreFuture<'a, ()>;
}
