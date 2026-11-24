use std::path::Path;

use eggplant_agent_storage::{
    ProviderAuthType, ProviderCredential, ProviderCredentialInfo, ProviderCredentialStore,
    StorageError, StoreFuture,
};
use sea_orm::{
    ActiveValue::Set, DatabaseConnection, EntityTrait, QueryOrder, QuerySelect,
    sea_query::OnConflict,
};

use crate::{connection, entity::agent_provider_credentials};

/// SQLite-backed provider credentials, separate from session transcript storage.
#[derive(Debug)]
pub struct SeaOrmProviderCredentialStore {
    db: DatabaseConnection,
}

impl SeaOrmProviderCredentialStore {
    /// Open an absolute SQLite file path and apply all agent migrations.
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Ok(Self {
            db: connection::connect(path).await?,
        })
    }
}

impl ProviderCredentialStore for SeaOrmProviderCredentialStore {
    fn get<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, Option<ProviderCredential>> {
        Box::pin(async move {
            agent_provider_credentials::Entity::find_by_id(provider_id)
                .one(&self.db)
                .await
                .map_err(backend)
                .map(|model| model.map(credential_record))
        })
    }

    fn list<'a>(&'a self) -> StoreFuture<'a, Vec<ProviderCredentialInfo>> {
        Box::pin(async move {
            agent_provider_credentials::Entity::find()
                .select_only()
                .column(agent_provider_credentials::Column::ProviderId)
                .column(agent_provider_credentials::Column::AuthType)
                .order_by_asc(agent_provider_credentials::Column::ProviderId)
                .into_tuple::<(String, agent_provider_credentials::AuthType)>()
                .all(&self.db)
                .await
                .map_err(backend)
                .map(|rows| {
                    rows.into_iter()
                        .map(|(provider_id, auth_type)| ProviderCredentialInfo {
                            provider_id,
                            auth_type: provider_auth_type(auth_type),
                        })
                        .collect()
                })
        })
    }

    fn upsert<'a>(&'a self, credential: ProviderCredential) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            credential.validate()?;
            agent_provider_credentials::Entity::insert(credential_active(credential))
                .on_conflict(
                    OnConflict::column(agent_provider_credentials::Column::ProviderId)
                        .update_columns([
                            agent_provider_credentials::Column::AuthType,
                            agent_provider_credentials::Column::ApiKey,
                            agent_provider_credentials::Column::AccessToken,
                            agent_provider_credentials::Column::RefreshToken,
                            agent_provider_credentials::Column::ExpiresAt,
                            agent_provider_credentials::Column::AccountId,
                            agent_provider_credentials::Column::CreatedAt,
                            agent_provider_credentials::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&self.db)
                .await
                .map_err(backend)?;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            agent_provider_credentials::Entity::delete_by_id(provider_id)
                .exec(&self.db)
                .await
                .map_err(backend)
                .map(|result| result.rows_affected > 0)
        })
    }
}

fn credential_active(credential: ProviderCredential) -> agent_provider_credentials::ActiveModel {
    let ProviderCredential {
        provider_id,
        auth_type,
        api_key,
        access_token,
        refresh_token,
        expires_at,
        account_id,
        created_at,
        updated_at,
    } = credential;
    agent_provider_credentials::ActiveModel {
        provider_id: Set(provider_id),
        auth_type: Set(entity_auth_type(auth_type)),
        api_key: Set(api_key),
        access_token: Set(access_token),
        refresh_token: Set(refresh_token),
        expires_at: Set(expires_at),
        account_id: Set(account_id),
        created_at: Set(created_at),
        updated_at: Set(updated_at),
    }
}

fn credential_record(model: agent_provider_credentials::Model) -> ProviderCredential {
    ProviderCredential {
        provider_id: model.provider_id,
        auth_type: provider_auth_type(model.auth_type),
        api_key: model.api_key,
        access_token: model.access_token,
        refresh_token: model.refresh_token,
        expires_at: model.expires_at,
        account_id: model.account_id,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }
}

fn entity_auth_type(auth_type: ProviderAuthType) -> agent_provider_credentials::AuthType {
    match auth_type {
        ProviderAuthType::OAuth => agent_provider_credentials::AuthType::OAuth,
        ProviderAuthType::ApiKey => agent_provider_credentials::AuthType::ApiKey,
    }
}

fn provider_auth_type(auth_type: agent_provider_credentials::AuthType) -> ProviderAuthType {
    match auth_type {
        agent_provider_credentials::AuthType::OAuth => ProviderAuthType::OAuth,
        agent_provider_credentials::AuthType::ApiKey => ProviderAuthType::ApiKey,
    }
}

fn backend(error: impl std::fmt::Display) -> StorageError {
    StorageError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use sea_orm::sqlx::Executor;

    use super::*;

    fn api_key(provider_id: &str, updated_at: i64) -> ProviderCredential {
        ProviderCredential {
            provider_id: provider_id.to_string(),
            auth_type: ProviderAuthType::ApiKey,
            api_key: Some("key-secret".to_string()),
            access_token: None,
            refresh_token: None,
            expires_at: None,
            account_id: None,
            created_at: updated_at,
            updated_at,
        }
    }

    fn oauth(provider_id: &str, updated_at: i64) -> ProviderCredential {
        ProviderCredential {
            provider_id: provider_id.to_string(),
            auth_type: ProviderAuthType::OAuth,
            api_key: None,
            access_token: Some("access-secret".to_string()),
            refresh_token: Some("refresh-secret".to_string()),
            expires_at: Some(1_800_000_000_000),
            account_id: Some("account-1".to_string()),
            created_at: updated_at,
            updated_at,
        }
    }

    #[tokio::test]
    async fn credentials_survive_reconnect_and_replace_authentication_methods() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested/agent.sqlite");
        let store = SeaOrmProviderCredentialStore::connect(&path).await.unwrap();
        let original = api_key("openai", 10);
        store.upsert(original.clone()).await.unwrap();
        assert_eq!(store.get("openai").await.unwrap(), Some(original));

        let replacement = oauth("openai", 20);
        store.upsert(replacement.clone()).await.unwrap();
        assert_eq!(
            store.get("openai").await.unwrap(),
            Some(replacement.clone())
        );
        assert_eq!(
            store.list().await.unwrap(),
            vec![ProviderCredentialInfo {
                provider_id: "openai".to_string(),
                auth_type: ProviderAuthType::OAuth,
            }]
        );
        drop(store);

        let reopened = SeaOrmProviderCredentialStore::connect(&path).await.unwrap();
        assert_eq!(reopened.get("openai").await.unwrap(), Some(replacement));
        assert!(path.is_file());
    }

    #[tokio::test]
    async fn list_is_sorted_and_delete_reports_whether_a_credential_existed() {
        let temp = tempfile::tempdir().unwrap();
        let store = SeaOrmProviderCredentialStore::connect(temp.path().join("agent.sqlite"))
            .await
            .unwrap();
        store.upsert(api_key("z-provider", 10)).await.unwrap();
        store.upsert(oauth("a-provider", 20)).await.unwrap();
        assert_eq!(
            store
                .list()
                .await
                .unwrap()
                .into_iter()
                .map(|credential| credential.provider_id)
                .collect::<Vec<_>>(),
            ["a-provider", "z-provider"]
        );

        assert!(store.delete("z-provider").await.unwrap());
        assert!(!store.delete("z-provider").await.unwrap());
        assert_eq!(store.get("z-provider").await.unwrap(), None);
        assert_eq!(store.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn invalid_credentials_are_rejected_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let store = SeaOrmProviderCredentialStore::connect(temp.path().join("agent.sqlite"))
            .await
            .unwrap();
        let mut invalid = oauth("openai", 10);
        invalid.access_token = None;
        assert!(matches!(
            store.upsert(invalid).await,
            Err(StorageError::InvalidCredential(_))
        ));
        assert_eq!(store.get("openai").await.unwrap(), None);

        let unsupported = store.db.get_sqlite_connection_pool().execute(
            "INSERT INTO agent_provider_credentials (provider_id, auth_type, created_at, updated_at) VALUES ('invalid', 'account', 1, 1)",
        )
        .await;
        assert!(unsupported.is_err());
    }

    #[tokio::test]
    async fn existing_session_databases_receive_the_provider_migration() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("agent.sqlite");
        let store = SeaOrmProviderCredentialStore::connect(&path).await.unwrap();
        let pool = store.db.get_sqlite_connection_pool();
        pool.execute(
            "DELETE FROM agent_schema_migrations WHERE name = 'm20261124_000002_create_agent_provider_credentials'",
        )
        .await
        .unwrap();
        pool.execute("DROP TABLE agent_provider_credentials")
            .await
            .unwrap();
        drop(store);

        let migrated = SeaOrmProviderCredentialStore::connect(&path).await.unwrap();
        migrated.upsert(api_key("openai", 10)).await.unwrap();
        assert!(migrated.get("openai").await.unwrap().is_some());
    }
}
