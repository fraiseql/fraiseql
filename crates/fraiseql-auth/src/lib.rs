//! Authentication, authorization, and session management for FraiseQL.
//!
//! Handles JWT validation, OAuth/OIDC flows, session management, and authorization.

#![forbid(unsafe_code)]
#![allow(missing_docs)] // Reason: migrated from fraiseql-server; docs are a separate effort
#![allow(clippy::module_name_repetitions)] // Reason: standard Rust API style
#![allow(clippy::must_use_candidate)] // Reason: builder methods return Self but callers chain
#![allow(clippy::missing_errors_doc)] // Reason: error types are self-documenting
#![allow(clippy::missing_panics_doc)] // Reason: panics are eliminated by design
#![allow(clippy::needless_pass_by_value)] // Reason: axum extractors require owned types
#![allow(clippy::unused_async)] // Reason: axum handler trait requires async fn
#![allow(clippy::similar_names)] // Reason: domain terms are conventional pairs
#![allow(clippy::unused_self)] // Reason: trait implementations require &self
#![allow(clippy::unnecessary_wraps)] // Reason: handler signatures must return Result
#![allow(clippy::too_many_lines)] // Reason: OAuth/OIDC flows are inherently verbose
#![allow(clippy::struct_excessive_bools)] // Reason: config structs use bools for feature flags
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology
#![allow(clippy::doc_markdown)] // Reason: technical terms don't need backtick wrapping
#![allow(clippy::return_self_not_must_use)] // Reason: builder pattern compatibility
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use in tests
#![allow(clippy::cast_possible_truncation)] // Reason: intentional casts for metrics
#![allow(clippy::cast_sign_loss)] // Reason: timestamp values are positive
#![allow(clippy::uninlined_format_args)] // Reason: named variables improve readability
#![allow(clippy::redundant_closure_for_method_calls)] // Reason: explicit closures clarify intent
#![allow(clippy::single_match_else)] // Reason: match with else clearer for variant extraction
#![allow(clippy::manual_let_else)] // Reason: match with early return clearer for multi-line extraction
#![allow(clippy::redundant_closure)] // Reason: explicit closures clarify argument transformation

pub mod audit_logger;
pub mod constant_time;
pub mod error;
pub mod error_sanitizer;
pub mod handlers;
pub mod jwks;
pub mod jwt;
pub mod middleware;
pub mod monitoring;
pub mod oauth;
pub mod oidc_provider;
pub mod operation_rbac;
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
pub use jwks::JwksCache;
pub use jwt::{Claims, JwtValidator, generate_hs256_token, generate_rs256_token};
pub use middleware::{AuthMiddleware, AuthenticatedUser};
pub use monitoring::{AuthEvent, AuthMetrics, OperationTimer};
pub use oauth::{
    ExternalAuthProvider, IdTokenClaims, NonceParameter, OAuth2Client, OAuth2ClientConfig,
    OAuthAuditEvent, OAuthSession, OIDCClient, OIDCProviderConfig, PKCEChallenge,
    ProviderFailoverManager, ProviderRegistry, ProviderType, StateParameter, TokenRefreshScheduler,
    TokenRefreshWorker, TokenRefresher,
};
pub use oidc_provider::OidcProvider;
pub use operation_rbac::{OperationPermission, RBACPolicy, Role};
pub use provider::{OAuthProvider, PkceChallenge, TokenResponse, UserInfo};
pub use providers::{AzureADOAuth, GitHubOAuth, GoogleOAuth, KeycloakOAuth, create_provider};
pub use proxy::ProxyConfig;
pub use rate_limiting::{KeyedRateLimiter, RateLimitConfig, RateLimiters};
pub use security_config::{
    AuditLoggingSettings, ErrorSanitizationSettings, RateLimitingSettings,
    SecurityConfigFromSchema, StateEncryptionSettings,
};
pub use security_init::{
    init_default_security_config, init_security_config, log_security_config,
    validate_security_config,
};
pub use session::{SessionData, SessionStore, TokenPair};
pub use session_postgres::PostgresSessionStore;
pub use state_encryption::{EncryptedState, StateEncryption, generate_state_encryption_key};
pub use state_store::{InMemoryStateStore, StateStore};
