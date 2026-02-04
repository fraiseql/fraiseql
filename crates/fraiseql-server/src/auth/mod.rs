// Authentication module
// Handles JWT validation, OAuth/OIDC flows, session management, and authorization

pub mod audit_logger;
pub mod constant_time;
pub mod error;
pub mod error_sanitizer;
pub mod handlers;
pub mod jwt;
pub mod middleware;
pub mod monitoring;
pub mod oauth;
pub mod oidc_provider;
pub mod operation_rbac;
pub mod provider;
pub mod providers;
pub mod rate_limiting;
pub mod security_config;
pub mod security_init;
pub mod session;
pub mod session_postgres;
pub mod state_encryption;
pub mod state_store;

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod audit_logging_tests;

#[cfg(test)]
mod error_sanitization_tests;

#[cfg(test)]
mod constant_time_tests;

#[cfg(test)]
mod state_encryption_tests;

#[cfg(test)]
mod rate_limiting_tests;

#[cfg(test)]
mod integration_security_tests;

#[cfg(test)]
mod oauth_tests;

pub use audit_logger::{
    AuditEntry, AuditEventType, AuditLogger, SecretType, StructuredAuditLogger, get_audit_logger,
    init_audit_logger,
};
pub use constant_time::ConstantTimeOps;
pub use error::{AuthError, Result};
pub use error_sanitizer::{
    AuthErrorSanitizer, Sanitizable, SanitizedError, messages as error_messages,
};
pub use handlers::{
    AuthCallbackQuery, AuthLogoutRequest, AuthRefreshRequest, AuthStartRequest, AuthState,
    auth_callback, auth_logout, auth_refresh, auth_start,
};
pub use jwt::{Claims, JwtValidator, generate_rs256_token, generate_hs256_token};
pub use middleware::{AuthMiddleware, AuthenticatedUser};
pub use monitoring::{AuthEvent, AuthMetrics, OperationTimer};
pub use oidc_provider::OidcProvider;
pub use operation_rbac::{OperationPermission, RBACPolicy, Role};
pub use provider::{OAuthProvider, PkceChallenge, TokenResponse, UserInfo};
pub use providers::{AzureADOAuth, GitHubOAuth, GoogleOAuth, KeycloakOAuth, create_provider};
pub use rate_limiting::{KeyedRateLimiter, RateLimitConfig, RateLimiters};
pub use session::{SessionData, SessionStore, TokenPair};
pub use session_postgres::PostgresSessionStore;
pub use state_encryption::{EncryptedState, StateEncryption, generate_state_encryption_key};
pub use state_store::{InMemoryStateStore, StateStore};
pub use security_config::{
    SecurityConfigFromSchema, AuditLoggingSettings, ErrorSanitizationSettings,
    RateLimitingSettings, StateEncryptionSettings,
};
pub use security_init::{
    init_security_config, init_default_security_config, log_security_config,
    validate_security_config,
};
pub use oauth::{
    OAuth2Client, OIDCClient, OIDCProviderConfig, IdTokenClaims,
    OAuthSession, ExternalAuthProvider, ProviderRegistry, ProviderType, OAuth2ClientConfig,
    PKCEChallenge, StateParameter, NonceParameter, TokenRefreshScheduler, ProviderFailoverManager,
    OAuthAuditEvent,
};
