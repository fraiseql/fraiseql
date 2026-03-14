//! External OAuth provider registry and session management.

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::{super::error::AuthError, client::OIDCProviderConfig};

/// External authentication provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    /// OAuth2 provider
    OAuth2,
    /// OIDC provider
    OIDC,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OAuth2 => write!(f, "oauth2"),
            Self::OIDC => write!(f, "oidc"),
        }
    }
}

/// OAuth session stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSession {
    /// Session ID
    pub id:               String,
    /// User ID (local system)
    pub user_id:          String,
    /// Provider type (oauth2, oidc)
    pub provider_type:    ProviderType,
    /// Provider name (Auth0, Google, etc.)
    pub provider_name:    String,
    /// Provider's user ID (sub claim)
    pub provider_user_id: String,
    /// Access token (encrypted)
    pub access_token:     String,
    /// Refresh token (encrypted), if available
    pub refresh_token:    Option<String>,
    /// When access token expires
    pub token_expiry:     DateTime<Utc>,
    /// Session creation time
    pub created_at:       DateTime<Utc>,
    /// Last time token was refreshed
    pub last_refreshed:   Option<DateTime<Utc>>,
}

impl OAuthSession {
    /// Create new OAuth session
    pub fn new(
        user_id: String,
        provider_type: ProviderType,
        provider_name: String,
        provider_user_id: String,
        access_token: String,
        token_expiry: DateTime<Utc>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id,
            provider_type,
            provider_name,
            provider_user_id,
            access_token,
            refresh_token: None,
            token_expiry,
            created_at: Utc::now(),
            last_refreshed: None,
        }
    }

    /// Check if session is expired
    pub fn is_expired(&self) -> bool {
        self.token_expiry <= Utc::now()
    }

    /// Check if session will be expired within grace period
    pub fn is_expiring_soon(&self, grace_seconds: i64) -> bool {
        self.token_expiry <= (Utc::now() + Duration::seconds(grace_seconds))
    }

    /// Update tokens after refresh
    pub fn refresh_tokens(&mut self, access_token: String, token_expiry: DateTime<Utc>) {
        self.access_token = access_token;
        self.token_expiry = token_expiry;
        self.last_refreshed = Some(Utc::now());
    }
}

/// External auth provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalAuthProvider {
    /// Provider ID
    pub id: String,
    /// Provider type (oauth2, oidc)
    pub provider_type: ProviderType,
    /// Provider name (Auth0, Google, Microsoft, Okta)
    pub provider_name: String,
    /// Client ID
    pub client_id: String,
    /// Client secret (should be fetched from vault)
    pub client_secret_vault_path: String,
    /// Provider configuration (OIDC)
    pub oidc_config: Option<OIDCProviderConfig>,
    /// OAuth2 configuration
    pub oauth2_config: Option<OAuth2ClientConfig>,
    /// Enabled flag
    pub enabled: bool,
    /// Requested scopes
    pub scopes: Vec<String>,
}

/// OAuth2 client configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuth2ClientConfig {
    /// Authorization endpoint
    pub authorization_endpoint: String,
    /// Token endpoint
    pub token_endpoint:         String,
    /// Use PKCE
    pub use_pkce:               bool,
}

impl ExternalAuthProvider {
    /// Create new external auth provider
    pub fn new(
        provider_type: ProviderType,
        provider_name: impl Into<String>,
        client_id: impl Into<String>,
        client_secret_vault_path: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            provider_type,
            provider_name: provider_name.into(),
            client_id: client_id.into(),
            client_secret_vault_path: client_secret_vault_path.into(),
            oidc_config: None,
            oauth2_config: None,
            enabled: true,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        }
    }

    /// Enable or disable provider
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set requested scopes
    pub fn set_scopes(&mut self, scopes: Vec<String>) {
        self.scopes = scopes;
    }
}

/// Provider registry managing multiple OAuth providers
#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    /// Map of providers by name
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    providers: Arc<std::sync::Mutex<HashMap<String, ExternalAuthProvider>>>,
}

impl ProviderRegistry {
    /// Create new provider registry
    pub fn new() -> Self {
        Self {
            providers: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Register provider
    pub fn register(&self, provider: ExternalAuthProvider) -> std::result::Result<(), AuthError> {
        let mut providers = self.providers.lock().map_err(|_| AuthError::Internal {
            message: "provider registry mutex poisoned".to_string(),
        })?;
        providers.insert(provider.provider_name.clone(), provider);
        Ok(())
    }

    /// Get provider by name
    pub fn get(&self, name: &str) -> std::result::Result<Option<ExternalAuthProvider>, AuthError> {
        let providers = self.providers.lock().map_err(|_| AuthError::Internal {
            message: "provider registry mutex poisoned".to_string(),
        })?;
        Ok(providers.get(name).cloned())
    }

    /// List all enabled providers
    pub fn list_enabled(&self) -> std::result::Result<Vec<ExternalAuthProvider>, AuthError> {
        let providers = self.providers.lock().map_err(|_| AuthError::Internal {
            message: "provider registry mutex poisoned".to_string(),
        })?;
        Ok(providers.values().filter(|p| p.enabled).cloned().collect())
    }

    /// Disable provider
    pub fn disable(&self, name: &str) -> std::result::Result<bool, AuthError> {
        let mut providers = self.providers.lock().map_err(|_| AuthError::Internal {
            message: "provider registry mutex poisoned".to_string(),
        })?;
        if let Some(provider) = providers.get_mut(name) {
            provider.set_enabled(false);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Enable provider
    pub fn enable(&self, name: &str) -> std::result::Result<bool, AuthError> {
        let mut providers = self.providers.lock().map_err(|_| AuthError::Internal {
            message: "provider registry mutex poisoned".to_string(),
        })?;
        if let Some(provider) = providers.get_mut(name) {
            provider.set_enabled(true);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
