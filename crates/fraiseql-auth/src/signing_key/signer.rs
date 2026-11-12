//! Signing the access tokens a session store mints.

use std::sync::Arc;

use super::LiveKeyRing;
use crate::{
    Claims,
    error::{AuthError, Result},
};

/// Signs the access tokens a [`PostgresSessionStore`](crate::PostgresSessionStore) mints.
pub trait AccessTokenSigner: Send + Sync + std::fmt::Debug {
    /// The compact JWS of `claims`.
    ///
    /// # Errors
    ///
    /// Any reason the token cannot be signed. No token is issued then.
    fn sign_access_token(&self, claims: &Claims) -> Result<String>;
}

/// Signs with the active key of a [`LiveKeyRing`], as an RFC 9068 access token
/// (`typ` `at+jwt`).
///
/// It refuses a token meant to live longer than `max_lifetime_secs`: the ring keeps a
/// retired key published only that long, so such a token would outlive its key and
/// stop validating early. A sign-in path that asks for more fails loudly instead.
#[derive(Debug, Clone)]
pub struct RingSigner {
    ring:              Arc<LiveKeyRing>,
    max_lifetime_secs: u64,
}

impl RingSigner {
    /// Sign with `ring`, refusing tokens meant to live longer than `max_lifetime_secs`.
    #[must_use]
    pub const fn new(ring: Arc<LiveKeyRing>, max_lifetime_secs: u64) -> Self {
        Self {
            ring,
            max_lifetime_secs,
        }
    }
}

impl AccessTokenSigner for RingSigner {
    fn sign_access_token(&self, claims: &Claims) -> Result<String> {
        let lifetime = claims.exp.saturating_sub(claims.iat);
        if lifetime > self.max_lifetime_secs {
            return Err(AuthError::ConfigError {
                message: format!(
                    "refusing to sign an access token meant to live {lifetime}s: the \
                     first-party issuer keeps a retired key published for tokens of at most \
                     {}s",
                    self.max_lifetime_secs
                ),
            });
        }
        self.ring.current()?.active().sign("at+jwt", claims)
    }
}
