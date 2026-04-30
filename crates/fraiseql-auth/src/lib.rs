//! Authentication, authorization, and session management for FraiseQL.
//!
//! Handles JWT validation, OAuth/OIDC flows, session management, and authorization.

#![forbid(unsafe_code)]
#![allow(clippy::needless_pass_by_value)] // Reason: axum extractors require owned types
#![allow(clippy::doc_markdown)] // Reason: technical terms (OAuth2, PKCE, OIDC, HMAC) throughout docs

pub mod account_linking;
pub mod audit;
pub mod constant_time;
pub mod error;
pub mod error_sanitizer;
pub mod handlers;
pub mod jwks;
pub mod jwt;
pub mod middleware;
pub mod monitoring;
pub mod multi_provider;
pub mod oauth;
pub mod oidc_provider;
pub mod oidc_server_client;
pub mod operation_rbac;
pub mod pkce;
pub mod provider;
pub mod providers;
pub mod proxy;
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
mod error_sanitization_tests;

#[cfg(test)]
mod constant_time_tests;

#[cfg(test)]
mod state_encryption_tests;

#[cfg(test)]
mod rate_limiting_tests;

#[cfg(test)]
mod integration_security_tests;

pub use audit::logger::{
    AuditEntry, AuditEventType, AuditExt, AuditLogger, SecretType, StructuredAuditLogger,
    get_audit_logger, init_audit_logger,
};
pub use account_linking::{InMemoryUserStore, LinkedIdentity, LocalUser, UserStore};
pub use constant_time::ConstantTimeOps;
pub use error::{AuthError, Result};
pub use error_sanitizer::{
    AuthErrorSanitizer, Sanitize, SanitizedError, messages as error_messages,
};
pub use handlers::{
    AuthCallbackQuery, AuthLogoutRequest, AuthRefreshRequest, AuthStartRequest, AuthState,
    auth_callback, auth_logout, auth_refresh, auth_start,
};
pub use jwks::{JwksCache, JwksError};
pub use jwt::{Claims, JwtValidator, generate_hs256_token, generate_rs256_token};
pub use middleware::{AuthMiddleware, AuthenticatedUser};
pub use multi_provider::{
    AuthTokenResponse, AuthorizeQuery, CallbackQuery, MultiProviderAuthState, ProvidersResponse,
    authorize, callback, list_providers,
};
pub use monitoring::{AuthEvent, AuthMetrics, OperationTimer};
pub use oauth::{
    AuthorizationRequest, ExternalAuthProvider, IdTokenClaims, NonceParameter, OAuth2Client,
    OAuth2ClientConfig, OAuthAuditEvent, OAuthSession, OIDCClient, OIDCProviderConfig,
    PKCEChallenge, ProviderFailoverManager, ProviderRegistry, ProviderType, StateParameter,
    TokenRefreshScheduler, TokenRefreshWorker, TokenRefresher,
};
pub use oidc_provider::OidcProvider;
pub use oidc_server_client::{OidcEndpoints, OidcServerClient, OidcTokenResponse};
pub use operation_rbac::{OperationPermission, RBACPolicy, Role};
pub use pkce::{ConsumedPkceState, PkceError, PkceStateStore};
pub use provider::{OAuthProvider, PkceChallenge, TokenResponse, UserInfo};
pub use providers::{AzureADOAuth, GitHubOAuth, GoogleOAuth, KeycloakOAuth, create_provider};
pub use proxy::ProxyConfig;
pub use rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter, RateLimiters};
pub use security_config::{
    AuditLoggingSettings, ErrorSanitizationSettings, RateLimitingSettings,
    SecurityConfigFromSchema, StateEncryptionSettings,
};
pub use security_init::{
    init_default_security_config, init_security_config, log_security_config,
    validate_security_config,
};
pub use session::{SessionData, SessionStore, TokenPair, unix_now};
pub use session_postgres::PostgresSessionStore;
pub use state_encryption::{
    DecryptionError, EncryptedState, EncryptionAlgorithm, KeyError, StateEncryption,
    StateEncryptionConfig, StateEncryptionService, generate_state_encryption_key,
};
pub use state_store::{InMemoryStateStore, StateStore};
