//! OAuth2 and OIDC authentication support with JWT validation,
//! provider discovery, and automatic user provisioning.

pub mod audit;
pub mod client;
pub mod failover;
pub mod pkce;
pub mod provider;
pub mod refresh;
pub mod types;

#[cfg(test)]
mod tests;

pub use audit::OAuthAuditEvent;
pub use client::{AuthorizationRequest, OAuth2Client, OIDCClient, OIDCProviderConfig};
pub use failover::ProviderFailoverManager;
pub use pkce::{NonceParameter, PKCEChallenge, StateParameter};
pub use provider::{
    ExternalAuthProvider, OAuth2ClientConfig, OAuthSession, ProviderRegistry, ProviderType,
};
pub use refresh::{TokenRefreshScheduler, TokenRefreshWorker, TokenRefresher};
pub use types::{IdTokenClaims, TokenResponse, UserInfo};
