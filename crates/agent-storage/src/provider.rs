use std::fmt;

use crate::{StorageError, StoreFuture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthType {
    OAuth,
    ApiKey,
}

/// Backend-neutral provider credential. Secret fields are redacted from `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCredential {
    pub provider_id: String,
    pub auth_type: ProviderAuthType,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub account_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ProviderCredential {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.provider_id.trim().is_empty() {
            return Err(invalid("provider ID must not be empty"));
        }
        match self.auth_type {
            ProviderAuthType::ApiKey => {
                require_secret(&self.api_key, "API key")?;
                if self.access_token.is_some()
                    || self.refresh_token.is_some()
                    || self.expires_at.is_some()
                    || self.account_id.is_some()
                {
                    return Err(invalid("API-key credentials cannot contain OAuth fields"));
                }
            }
            ProviderAuthType::OAuth => {
                require_secret(&self.access_token, "OAuth access token")?;
                if self
                    .refresh_token
                    .as_deref()
                    .is_some_and(|token| token.trim().is_empty())
                {
                    return Err(invalid("OAuth refresh token must not be empty"));
                }
                if self.api_key.is_some() {
                    return Err(invalid("OAuth credentials cannot contain an API key"));
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for ProviderCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderCredential")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialInfo {
    pub provider_id: String,
    pub auth_type: ProviderAuthType,
}

/// Provider authentication persistence, independent of session transcript storage.
pub trait ProviderCredentialStore: std::fmt::Debug + Send + Sync {
    fn get<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, Option<ProviderCredential>>;

    /// List configured methods without reading or returning credential secrets.
    fn list<'a>(&'a self) -> StoreFuture<'a, Vec<ProviderCredentialInfo>>;

    /// Insert or replace the single credential associated with a provider.
    fn upsert<'a>(&'a self, credential: ProviderCredential) -> StoreFuture<'a, ()>;

    /// Remove a credential and return whether one existed.
    fn delete<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, bool>;
}

fn require_secret(value: &Option<String>, label: &str) -> Result<(), StorageError> {
    if value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(());
    }
    Err(invalid(format!("{label} must not be empty")))
}

fn invalid(message: impl Into<String>) -> StorageError {
    StorageError::InvalidCredential(message.into())
}

fn redacted(secret: &Option<String>) -> Option<&'static str> {
    secret.as_ref().map(|_| "[redacted]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(auth_type: ProviderAuthType) -> ProviderCredential {
        ProviderCredential {
            provider_id: "openai".to_string(),
            auth_type,
            base_url: None,
            api_key: None,
            access_token: None,
            refresh_token: None,
            expires_at: None,
            account_id: None,
            created_at: 1,
            updated_at: 2,
        }
    }

    #[test]
    fn validates_method_specific_fields_without_exposing_secrets() {
        let mut api_key = credential(ProviderAuthType::ApiKey);
        assert!(matches!(
            api_key.validate(),
            Err(StorageError::InvalidCredential(_))
        ));
        api_key.api_key = Some("key-secret".to_string());
        api_key.access_token = Some("unexpected".to_string());
        assert!(api_key.validate().is_err());
        api_key.access_token = None;
        assert!(api_key.validate().is_ok());

        let mut oauth = credential(ProviderAuthType::OAuth);
        assert!(oauth.validate().is_err());
        oauth.access_token = Some("access-secret".to_string());
        oauth.refresh_token = Some("refresh-secret".to_string());
        assert!(oauth.validate().is_ok());

        let output = format!("{oauth:?}");
        assert!(!output.contains("access-secret"));
        assert!(!output.contains("refresh-secret"));
        assert_eq!(output.matches("[redacted]").count(), 2);
    }
}
