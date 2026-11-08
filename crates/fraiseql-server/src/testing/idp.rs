//! A local OIDC identity provider for unit tests: the shared ES256 test issuer, and a
//! validator pointed at its JWKS through the production constructor.

use std::sync::Arc;

use fraiseql_core::security::{OidcConfig, OidcValidator};
use fraiseql_test_support::issuer::{Claims, TestIssuer};

/// A running issuer. Dropping it stops the server.
pub struct TestIdp {
    issuer: TestIssuer,
}

impl TestIdp {
    /// Start the issuer.
    pub async fn start() -> Self {
        Self {
            issuer: TestIssuer::start().await,
        }
    }

    /// The `[auth]` section of a deployment that trusts this issuer, for its issuer
    /// and audience.
    ///
    /// The JWKS URI is left unset, so a server built from it finds the key set the
    /// way a deployment does: by discovery from the issuer.
    #[must_use]
    pub fn oidc_config(&self, required: bool) -> OidcConfig {
        OidcConfig {
            issuer: Some(self.issuer.issuer().to_string()),
            audience: Some(self.issuer.audience().to_string()),
            required,
            allowed_algorithms: vec!["ES256".to_string()],
            ..OidcConfig::default()
        }
    }

    /// A validator that trusts this issuer's keys, for its issuer and audience.
    ///
    /// # Panics
    ///
    /// If the validator refuses the loopback `jwks_uri`.
    #[must_use]
    pub fn validator(&self, required: bool) -> Arc<OidcValidator> {
        Arc::new(
            OidcValidator::with_jwks_uri(self.oidc_config(required), &self.issuer.jwks_uri())
                .expect("a loopback jwks_uri"),
        )
    }

    /// The default claims for `sub`, ready for a test to add the claims it needs before
    /// [`mint`](Self::mint).
    #[must_use]
    pub fn claims(&self, sub: &str) -> Claims {
        self.issuer.claims().set("sub", sub)
    }

    /// A token carrying exactly `claims`, signed with this issuer's key.
    #[must_use]
    pub fn mint(&self, claims: &Claims) -> String {
        self.issuer.mint(claims)
    }

    /// A token for `sub` carrying `jti`, signed with this issuer's key.
    #[must_use]
    pub fn token(&self, sub: &str, jti: &str) -> String {
        self.mint(&self.claims(sub).set("jti", jti))
    }

    /// A token for `sub` carrying `jti`, naming this issuer's `kid` but signed by a
    /// key it does not publish.
    #[must_use]
    pub fn foreign_token(&self, sub: &str, jti: &str) -> String {
        self.issuer.foreign_token(&self.issuer.claims().set("sub", sub).set("jti", jti))
    }
}

#[cfg(test)]
#[path = "idp_tests.rs"]
mod tests;
