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

#[cfg(test)]
mod tests {
    use super::*;

    // --- ProviderType tests ---

    #[test]
    fn test_provider_type_display() {
        assert_eq!(ProviderType::OAuth2.to_string(), "oauth2");
        assert_eq!(ProviderType::OIDC.to_string(), "oidc");
    }

    // --- OAuthSession tests ---

    #[test]
    fn test_session_is_not_expired_on_creation() {
        let session = OAuthSession::new(
            "user_1".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub".to_string(),
            "at_fresh".to_string(),
            Utc::now() + Duration::hours(1),
        );
        assert!(!session.is_expired(), "freshly created session must not be expired");
    }

    #[test]
    fn test_session_is_expiring_soon() {
        let session = OAuthSession::new(
            "user_1".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub".to_string(),
            "at".to_string(),
            Utc::now() + Duration::seconds(30),
        );
        assert!(
            session.is_expiring_soon(60),
            "session expiring in 30s must be considered expiring soon with grace=60"
        );
        assert!(
            !session.is_expiring_soon(10),
            "session expiring in 30s must not be considered expiring soon with grace=10"
        );
    }

    #[test]
    fn test_session_refresh_tokens_updates_fields() {
        let mut session = OAuthSession::new(
            "user_1".to_string(),
            ProviderType::OIDC,
            "auth0".to_string(),
            "auth0|sub".to_string(),
            "old_at".to_string(),
            Utc::now() + Duration::hours(1),
        );
        let new_expiry = Utc::now() + Duration::hours(2);
        session.refresh_tokens("new_at".to_string(), new_expiry);

        assert_eq!(session.access_token, "new_at");
        assert_eq!(session.token_expiry, new_expiry);
        assert!(session.last_refreshed.is_some());
    }

    // --- ExternalAuthProvider tests ---

    #[test]
    fn test_external_provider_defaults() {
        let provider =
            ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "client_id", "vault/secret");
        assert!(provider.enabled, "new provider must be enabled by default");
        assert_eq!(provider.scopes, vec!["openid", "profile", "email"]);
        assert!(provider.oidc_config.is_none());
        assert!(provider.oauth2_config.is_none());
    }

    #[test]
    fn test_external_provider_set_enabled() {
        let mut provider =
            ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id", "vault/path");
        provider.set_enabled(false);
        assert!(!provider.enabled);
        provider.set_enabled(true);
        assert!(provider.enabled);
    }

    #[test]
    fn test_external_provider_set_scopes() {
        let mut provider =
            ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id", "vault/path");
        provider.set_scopes(vec!["openid".to_string()]);
        assert_eq!(provider.scopes, vec!["openid"]);
    }

    // --- ProviderRegistry tests ---

    #[test]
    fn test_registry_register_and_get() {
        let registry = ProviderRegistry::new();
        let provider =
            ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id", "vault/path");
        registry
            .register(provider.clone())
            .expect("register must succeed");
        let retrieved = registry.get("auth0").expect("get must succeed");
        assert_eq!(retrieved, Some(provider));
    }

    #[test]
    fn test_registry_get_nonexistent_returns_none() {
        let registry = ProviderRegistry::new();
        let retrieved = registry.get("nonexistent").expect("get must succeed");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_registry_list_enabled_filters_disabled() {
        let registry = ProviderRegistry::new();
        let p1 = ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id1", "path1");
        let mut p2 = ExternalAuthProvider::new(ProviderType::OAuth2, "google", "id2", "path2");
        p2.set_enabled(false);
        registry.register(p1).expect("register must succeed");
        registry.register(p2).expect("register must succeed");

        let enabled = registry.list_enabled().expect("list_enabled must succeed");
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].provider_name, "auth0");
    }

    #[test]
    fn test_registry_disable_and_enable() {
        let registry = ProviderRegistry::new();
        let provider =
            ExternalAuthProvider::new(ProviderType::OIDC, "auth0", "id", "path");
        registry.register(provider).expect("register must succeed");

        assert!(registry.disable("auth0").expect("disable must succeed"));
        let p = registry.get("auth0").expect("get must succeed").expect("provider must exist");
        assert!(!p.enabled);

        assert!(registry.enable("auth0").expect("enable must succeed"));
        let p = registry.get("auth0").expect("get must succeed").expect("provider must exist");
        assert!(p.enabled);
    }

    #[test]
    fn test_registry_disable_nonexistent_returns_false() {
        let registry = ProviderRegistry::new();
        assert!(!registry.disable("nonexistent").expect("disable must succeed"));
    }
}
