//! Authentication, authorization, and session management for FraiseQL.
//!
//! Handles JWT validation, OAuth/OIDC flows, session management, and authorization.

#![forbid(unsafe_code)]
// module_name_repetitions, must_use_candidate, similar_names, unnecessary_wraps:
// allowed at workspace level (Cargo.toml [workspace.lints.clippy]).
#![allow(clippy::missing_errors_doc)] // Reason: error types are self-documenting
#![allow(clippy::missing_panics_doc)] // Reason: panics are eliminated by design
#![allow(clippy::needless_pass_by_value)] // Reason: axum extractors require owned types
#![allow(clippy::unused_async)] // Reason: axum handler trait requires async fn
#![allow(clippy::unused_self)] // Reason: trait implementations require &self
//  clippy::too_many_lines — removed from module level; applied per-function where warranted.
//  clippy::wildcard_imports — removed from module level; applied per-site on `use super::*`.
#![allow(clippy::struct_excessive_bools)] // Reason: config structs use bools for feature flags
#![allow(clippy::struct_field_names)] // Reason: field prefixes match domain terminology
#![allow(clippy::doc_markdown)] // Reason: technical terms don't need backtick wrapping
#![allow(clippy::return_self_not_must_use)] // Reason: builder pattern compatibility
#![allow(clippy::items_after_statements)] // Reason: helper structs near point of use in tests
#![allow(clippy::cast_possible_truncation)] // Reason: intentional casts for metrics
#![allow(clippy::cast_sign_loss)] // Reason: timestamp values are positive
#![allow(clippy::uninlined_format_args)] // Reason: named variables improve readability
#![allow(clippy::redundant_closure_for_method_calls)] // Reason: explicit closures clarify intent
#![allow(clippy::single_match_else)] // Reason: match with else clearer for variant extraction
#![allow(clippy::manual_let_else)] // Reason: match with early return clearer for multi-line extraction
#![allow(clippy::redundant_closure)] // Reason: explicit closures clarify argument transformation
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used
#![allow(clippy::format_push_string)] // Reason: format! with push_str clearer than write!
#![allow(clippy::match_same_arms)] // Reason: explicit arms document per-variant intent
#![allow(clippy::cast_possible_wrap)] // Reason: values are within i64 range by design
#![allow(clippy::useless_format)] // Reason: format! used for consistency with other branches
#![allow(clippy::cast_precision_loss)] // Reason: acceptable precision for metrics/timing
#![allow(clippy::redundant_clone)] // Reason: explicit clone at API boundaries for clarity
#![allow(clippy::missing_fields_in_debug)] // Reason: sensitive fields excluded from Debug
#![allow(clippy::map_unwrap_or)] // Reason: map().unwrap_or() reads left-to-right
#![allow(clippy::cast_lossless)] // Reason: explicit cast preferred for readability
#![allow(clippy::unnecessary_map_or)] // Reason: map_or reads left-to-right at call site
#![allow(clippy::single_char_pattern)] // Reason: single-char str patterns are conventional
#![allow(clippy::float_cmp)] // Reason: exact float comparison intentional in timing tests
#![allow(clippy::ignored_unit_patterns)] // Reason: _ pattern in Ok(()) destructuring
#![allow(clippy::default_trait_access)] // Reason: Type::default() clearer than Default::default()

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
pub mod oidc_server_client;
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
    AuthorizationRequest, ExternalAuthProvider, IdTokenClaims, NonceParameter, OAuth2Client,
    OAuth2ClientConfig, OAuthAuditEvent, OAuthSession, OIDCClient, OIDCProviderConfig, PKCEChallenge,
    ProviderFailoverManager, ProviderRegistry, ProviderType, StateParameter, TokenRefreshScheduler,
    TokenRefreshWorker, TokenRefresher,
};
pub use oidc_provider::OidcProvider;
pub use operation_rbac::{OperationPermission, RBACPolicy, Role};
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
pub use session::{SessionData, SessionStore, TokenPair};
pub use session_postgres::PostgresSessionStore;
pub use oidc_server_client::{OidcEndpoints, OidcServerClient, OidcTokenResponse};
pub use pkce::{ConsumedPkceState, PkceError, PkceStateStore};
pub use state_encryption::{
    DecryptionError, EncryptedState, EncryptionAlgorithm, KeyError, StateEncryption,
    StateEncryptionConfig, StateEncryptionService, generate_state_encryption_key,
};
pub use state_store::{InMemoryStateStore, StateStore};
