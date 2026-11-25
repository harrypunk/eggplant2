use std::path::Path;

use eggplant_agent_storage::{ModelCatalog, ModelCatalogStore, StorageError, StoreFuture};
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait, sea_query::OnConflict};

use crate::{connection, entity::agent_model_catalogs};

/// SQLite-backed provider model catalogs, independent of credential records.
#[derive(Debug)]
pub struct SeaOrmModelCatalogStore {
    db: DatabaseConnection,
}

impl SeaOrmModelCatalogStore {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Ok(Self {
            db: connection::connect(path).await?,
        })
    }
}

impl ModelCatalogStore for SeaOrmModelCatalogStore {
    fn get<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, Option<ModelCatalog>> {
        Box::pin(async move {
            agent_model_catalogs::Entity::find_by_id(provider_id)
                .one(&self.db)
                .await
                .map_err(backend)?
                .map(catalog_record)
                .transpose()
        })
    }

    fn upsert<'a>(&'a self, catalog: ModelCatalog) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let active = catalog_active(catalog)?;
            agent_model_catalogs::Entity::insert(active)
                .on_conflict(
                    OnConflict::column(agent_model_catalogs::Column::ProviderId)
                        .update_columns([
                            agent_model_catalogs::Column::ModelsJson,
                            agent_model_catalogs::Column::CheckedAt,
                            agent_model_catalogs::Column::LastModified,
                            agent_model_catalogs::Column::Etag,
                        ])
                        .to_owned(),
                )
                .exec(&self.db)
                .await
                .map_err(backend)?;
            Ok(())
        })
    }
}

fn catalog_active(
    catalog: ModelCatalog,
) -> Result<agent_model_catalogs::ActiveModel, StorageError> {
    let models_json = serde_json::to_string(&catalog.models).map_err(backend)?;
    Ok(agent_model_catalogs::ActiveModel {
        provider_id: Set(catalog.provider_id),
        models_json: Set(models_json),
        checked_at: Set(catalog.checked_at),
        last_modified: Set(catalog.last_modified),
        etag: Set(catalog.etag),
    })
}

fn catalog_record(model: agent_model_catalogs::Model) -> Result<ModelCatalog, StorageError> {
    let models = serde_json::from_str(&model.models_json).map_err(backend)?;
    Ok(
        ModelCatalog::new(model.provider_id, models, model.checked_at)?
            .with_validators(model.last_modified, model.etag),
    )
}

fn backend(error: impl std::fmt::Display) -> StorageError {
    StorageError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn catalogs_are_normalized_replaced_and_restored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.sqlite");
        let store = SeaOrmModelCatalogStore::connect(&path).await.unwrap();
        let catalog = ModelCatalog::new(
            "openai",
            vec![
                "gpt-b".to_string(),
                "gpt-a".to_string(),
                "gpt-a".to_string(),
            ],
            100,
        )
        .unwrap()
        .with_validators(Some(90), Some("etag-1".to_string()));
        store.upsert(catalog.clone()).await.unwrap();
        assert_eq!(store.get("openai").await.unwrap(), Some(catalog));

        let replacement = ModelCatalog::new("openai", vec!["gpt-c".to_string()], 200)
            .unwrap()
            .with_validators(Some(190), Some("etag-2".to_string()));
        store.upsert(replacement.clone()).await.unwrap();
        drop(store);

        let reopened = SeaOrmModelCatalogStore::connect(&path).await.unwrap();
        assert_eq!(reopened.get("openai").await.unwrap(), Some(replacement));
        assert_eq!(reopened.get("missing").await.unwrap(), None);
    }
}
