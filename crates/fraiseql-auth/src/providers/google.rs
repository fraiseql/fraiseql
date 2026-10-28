//! Google OAuth / OIDC provider implementation using Google Identity Services.
use async_trait::async_trait;

use crate::{
    error::Result,
    oidc_provider::OidcProvider,
    provider::{OAuthProvider, TokenResponse, UserInfo},
};

/// Google OAuth provider wrapper
///
/// Google Identity Services over [`OidcProvider`]: the identity is the OIDC userinfo, as
/// Google returns it. No role or organisation is derived from it.
#[derive(Debug)]
pub struct GoogleOAuth {
    oidc: OidcProvider,
}

impl GoogleOAuth {
    /// Create a new Google OAuth provider
    ///
    /// # Arguments
    /// * `client_id` - Google OAuth client ID (from Google Cloud Console)
    /// * `client_secret` - Google OAuth client secret
    /// * `redirect_uri` - Redirect URI after authentication (e.g., "http://localhost:8000/auth/callback")
    ///
    /// # Errors
    ///
    /// Returns `AuthError` if OIDC discovery against Google fails.
    pub async fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    ) -> Result<Self> {
        Self::with_issuer(client_id, client_secret, redirect_uri, "https://accounts.google.com")
            .await
    }

    /// Create a Google OAuth provider against an explicit OIDC issuer —
    /// Google-compatible stand-ins or a stub `IdP` in tests (#368,
    /// `[auth.social.google] discovery_url`). Discovery is fetched at boot
    /// under the shared SSRF guards, exactly like [`Self::new`].
    ///
    /// # Errors
    ///
    /// Returns `AuthError` if the issuer fails SSRF validation or discovery fails.
    pub async fn with_issuer(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        issuer_url: &str,
    ) -> Result<Self> {
        let oidc =
            OidcProvider::new("google", issuer_url, &client_id, &client_secret, &redirect_uri)
                .await?;

        Ok(Self { oidc })
    }
}

// Reason: OAuthProvider is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OAuthProvider for GoogleOAuth {
    fn name(&self) -> &'static str {
        "google"
    }

    fn authorization_url(&self, state: &str) -> String {
        // Add additional scopes for Workspace directory access if needed
        // Note: This requires configuration of the authorization URL with scopes
        self.oidc.authorization_url(state)
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        self.oidc.exchange_code(code).await
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        self.oidc.user_info(access_token).await
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        self.oidc.refresh_token(refresh_token).await
    }

    async fn revoke_token(&self, token: &str) -> Result<()> {
        self.oidc.revoke_token(token).await
    }
}
