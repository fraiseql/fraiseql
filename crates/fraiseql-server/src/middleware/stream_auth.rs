//! Mid-delivery authorization for long-lived responses.
//!
//! The HTTP auth middleware validates a principal **once**, at request entry. That is
//! the whole of the guarantee for a buffered response, which is over in milliseconds.
//! It is not the whole of the guarantee for a response that stays open: a subscription
//! runs for an unbounded duration, and a `@stream` delivery runs for as long as the
//! result set takes. Over that window a token can expire, be revoked individually, or
//! be caught by a `revoke-all` epoch — and a transport that only checked at entry keeps
//! serving the revoked principal until the connection happens to end.
//!
//! [`StreamAuthGuard`] is the one implementation of that re-check, shared by every
//! long-lived transport, so the two cannot answer the question differently.

use std::sync::Arc;

use fraiseql_core::security::SecurityContext;

use crate::{middleware::oidc_auth::SessionTokenClaims, token_revocation::TokenRevocationManager};

/// The per-delivery authorization guard for a long-lived response (#771, #958).
///
/// - **Expiry** is a clock comparison against the principal's `expires_at` (the JWT `exp` claim;
///   service-account principals carry their mint-time ceiling).
/// - **Revocation** consults the configured revocation store (never the `IdP`) with the token's
///   `jti`/`iat` claims, exactly like the HTTP middleware does at request time. It applies only to
///   JWT principals (the claims extension is the marker); a store outage follows the manager's
///   configured fail-open/fail-closed posture.
///
/// The claims extension being the marker is why an auth layer that authenticates a
/// principal without inserting `SessionTokenClaims` silently disables the revocation
/// half of this guard for every connection it admits — which is what `[auth_hs256]`
/// did before #1112.
#[derive(Clone)]
pub struct StreamAuthGuard {
    /// When the principal's token expires. `None` for anonymous connections.
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The principal's subject, for the revocation `revoke-all` epoch check.
    sub:        Option<String>,
    /// The validated bearer token's `jti`/`iat` claims. `None` when the
    /// connection did not authenticate with a JWT — which also disables the
    /// revocation re-check for it.
    claims:     Option<SessionTokenClaims>,
    /// The revocation store to consult, when configured.
    revocation: Option<Arc<TokenRevocationManager>>,
}

impl StreamAuthGuard {
    /// Build a guard for `principal`, with the token claims and revocation manager the
    /// request carried.
    #[must_use]
    pub fn new(
        principal: Option<&SecurityContext>,
        claims: Option<SessionTokenClaims>,
        revocation: Option<Arc<TokenRevocationManager>>,
    ) -> Self {
        Self {
            expires_at: principal.map(|p| p.expires_at),
            sub: principal.map(|p| p.user_id.to_string()),
            // Revocation is a JWT concern: without decoded token claims there is
            // no jti/iat to check (anonymous or service-account connection).
            revocation: if claims.is_some() { revocation } else { None },
            claims,
        }
    }

    /// Whether any mid-delivery re-check applies to this principal.
    #[must_use]
    pub const fn applies(&self) -> bool {
        self.expires_at.is_some()
    }

    /// Cheap per-delivery check: has the principal's token expired?
    #[must_use]
    pub fn expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp <= chrono::Utc::now())
    }

    /// The full re-check: expiry plus revocation.
    ///
    /// # Errors
    ///
    /// Returns the termination reason — the string the transport reports to the client
    /// — when the principal has expired, has been revoked, lacks a required `jti`, or
    /// could not be checked under a fail-closed revocation posture.
    pub async fn check(&self) -> Result<(), &'static str> {
        if self.expired() {
            return Err("Token expired");
        }
        if let (Some(revocation), Some(sub), Some(claims)) =
            (self.revocation.as_ref(), self.sub.as_deref(), self.claims.as_ref())
        {
            use crate::token_revocation::TokenRejection;
            match revocation.check_token(claims.jti.as_deref(), sub, claims.iat).await {
                Ok(()) => {},
                Err(TokenRejection::Revoked) => return Err("Token revoked"),
                Err(TokenRejection::MissingJti) => return Err("Token lacks required jti claim"),
                // The manager already applied its fail-open posture internally; an
                // error here means fail-closed is configured — terminate.
                Err(TokenRejection::StoreUnavailable) => {
                    return Err("Revocation store unavailable");
                },
            }
        }
        Ok(())
    }
}
