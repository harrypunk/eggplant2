use std::fmt;

use sea_orm::entity::prelude::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(
    rs_type = "String",
    db_type = "String(StringLen::None)",
    enum_name = "auth_type"
)]
pub enum AuthType {
    #[sea_orm(string_value = "oauth")]
    OAuth,
    #[sea_orm(string_value = "api_key")]
    ApiKey,
}

/// One locally stored credential per provider.
///
/// `auth_type` is either `oauth` or `api_key`. OAuth rows use the token fields;
/// API-key rows use `api_key`. Provider catalog entries and login menus remain
/// application data rather than credential rows.
#[derive(Clone, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_provider_credentials")]
pub struct Model {
    /// Stable provider catalog ID, such as `openai` or `anthropic`.
    #[sea_orm(primary_key)]
    pub provider_id: String,
    /// Credential discriminator stored as `oauth` or `api_key`.
    pub auth_type: AuthType,
    /// Optional OpenAI-compatible endpoint or provider-specific base URL.
    pub base_url: Option<String>,
    /// Secret used by API-key authentication.
    pub api_key: Option<String>,
    /// Short-lived bearer token used by account/OAuth authentication.
    pub access_token: Option<String>,
    /// Token used to refresh OAuth access, when the provider supplies one.
    pub refresh_token: Option<String>,
    /// OAuth access-token expiry as Unix milliseconds, if it expires.
    pub expires_at: Option<i64>,
    /// Optional provider account identity required by some OAuth APIs.
    pub account_id: Option<String>,
    /// Creation time as Unix milliseconds.
    pub created_at: i64,
    /// Last credential replacement or refresh as Unix milliseconds.
    pub updated_at: i64,
}

impl fmt::Debug for Model {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Model")
            .field("provider_id", &self.provider_id)
            .field("auth_type", &self.auth_type)
            .field("base_url", &self.base_url)
            .field("api_key", &redacted(&self.api_key))
            .field("access_token", &redacted(&self.access_token))
            .field("refresh_token", &redacted(&self.refresh_token))
            .field("expires_at", &self.expires_at)
            .field("account_id", &self.account_id)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

fn redacted(secret: &Option<String>) -> Option<&'static str> {
    secret.as_ref().map(|_| "[redacted]")
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_type_has_stable_database_values() {
        assert_eq!(AuthType::OAuth.to_value(), "oauth");
        assert_eq!(AuthType::ApiKey.to_value(), "api_key");
        assert_eq!(
            AuthType::try_from_value(&"oauth".to_string()).unwrap(),
            AuthType::OAuth
        );
        assert_eq!(
            AuthType::try_from_value(&"api_key".to_string()).unwrap(),
            AuthType::ApiKey
        );
        assert!(AuthType::try_from_value(&"account".to_string()).is_err());
    }

    #[test]
    fn debug_output_redacts_credentials() {
        let model = Model {
            provider_id: "openai".to_string(),
            auth_type: AuthType::OAuth,
            base_url: None,
            api_key: Some("api-secret".to_string()),
            access_token: Some("access-secret".to_string()),
            refresh_token: Some("refresh-secret".to_string()),
            expires_at: Some(1_800_000_000_000),
            account_id: Some("account-1".to_string()),
            created_at: 1,
            updated_at: 2,
        };

        let output = format!("{model:?}");
        assert!(output.contains("openai"));
        assert!(output.contains("account-1"));
        assert!(!output.contains("api-secret"));
        assert!(!output.contains("access-secret"));
        assert!(!output.contains("refresh-secret"));
        assert_eq!(output.matches("[redacted]").count(), 3);
    }
}
