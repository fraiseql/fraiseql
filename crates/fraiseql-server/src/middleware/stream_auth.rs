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
//! long-lived transport, so the two cannot answer the question differently. Beyond the
//! token, it re-identifies the subject: a subject `[identity.enrichment]` no longer
//! resolves stops being served, as a new request from it would be refused.

use std::sync::Arc;

use axum::http::HeaderMap;
use fraiseql_core::security::SecurityContext;

use crate::{
    admission::AdmissionPolicy, middleware::oidc_auth::SessionTokenClaims,
    routes::graphql::AppState, token_revocation::TokenRevocationManager,
};

/// Why a long-lived response stops being served. Each transport words it on its own wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRefusal {
    /// The token no longer authenticates: expired, revoked, missing a required `jti`, or
    /// uncheckable under a fail-closed revocation posture.
    Unauthenticated(&'static str),
    /// The subject is no longer admitted: the identity resolver denies it.
    Forbidden(&'static str),
    /// The identity resolver could not answer, so the subject cannot be admitted.
    Unavailable(&'static str),
}

impl StreamRefusal {
    /// The reason the client is told.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unauthenticated(reason) | Self::Forbidden(reason) | Self::Unavailable(reason) => {
                reason
            },
        }
    }
}

/// What re-identifies a subject: the principal as the auth layer produced it, before
/// any enrichment, and the request it was admitted with.
#[derive(Clone)]
struct IdentityRecheck {
    app:           AppState,
    headers:       HeaderMap,
    authenticated: SecurityContext,
    policy:        AdmissionPolicy,
}

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
/// half of this guard for every connection it admits (#1112).
#[derive(Clone)]
pub struct StreamAuthGuard {
    /// When the principal's token expires. `None` for anonymous connections.
    expires_at: Option<fraiseql_time::Timestamp>,
    /// The principal's subject, for the revocation `revoke-all` epoch check.
    sub:        Option<String>,
    /// The validated bearer token's `jti`/`iat` claims. `None` when the
    /// connection did not authenticate with a JWT — which also disables the
    /// revocation re-check for it.
    claims:     Option<SessionTokenClaims>,
    /// The revocation store to consult, when configured.
    revocation: Option<Arc<TokenRevocationManager>>,
    /// The subject's identity, re-resolved on every full re-check. `None` for an
    /// anonymous connection, or a transport mounted without application state.
    identity:   Option<IdentityRecheck>,
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
            identity: None,
        }
    }

    /// Re-identify the subject on every full re-check, as `policy` admitted it.
    ///
    /// `authenticated` is the principal the auth layer produced, **before** enrichment:
    /// an enriched principal already carries what a resolve would merge, and resolving it
    /// again would only confirm the answer it was admitted with.
    #[must_use]
    pub fn with_identity(
        mut self,
        app: Option<&AppState>,
        headers: &HeaderMap,
        authenticated: Option<SecurityContext>,
        policy: AdmissionPolicy,
    ) -> Self {
        self.identity = app.zip(authenticated).map(|(app, authenticated)| IdentityRecheck {
            app: app.clone(),
            headers: headers.clone(),
            authenticated,
            policy,
        });
        self
    }

    /// Whether any mid-delivery re-check applies to this principal.
    #[must_use]
    pub const fn applies(&self) -> bool {
        self.expires_at.is_some()
    }

    /// Cheap per-delivery check: has the principal's token expired?
    #[must_use]
    pub fn expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp <= fraiseql_time::Timestamp::now())
    }

    /// The full re-check: expiry, revocation, and the subject's identity.
    ///
    /// # Errors
    ///
    /// [`StreamRefusal::Unauthenticated`] when the principal has expired, has been
    /// revoked, lacks a required `jti`, or could not be checked under a fail-closed
    /// revocation posture; [`StreamRefusal::Forbidden`] when the identity resolver now
    /// denies the subject; [`StreamRefusal::Unavailable`] when it cannot answer.
    pub async fn check(&self) -> Result<(), StreamRefusal> {
        if self.expired() {
            return Err(StreamRefusal::Unauthenticated(crate::admission::TOKEN_EXPIRED));
        }
        if let (Some(revocation), Some(sub), Some(claims)) =
            (self.revocation.as_ref(), self.sub.as_deref(), self.claims.as_ref())
        {
            // The manager already applied its fail-open posture internally; an error here
            // (a revoked token, a missing required jti, or a store outage under
            // fail-closed) terminates.
            revocation
                .check_token(claims.jti.as_deref(), sub, claims.iat)
                .await
                .map_err(|rejection| StreamRefusal::Unauthenticated(rejection.reason()))?;
        }
        if let Some(identity) = self.identity.as_ref() {
            use fraiseql_core::security::EnrichmentOutcome;

            use crate::admission::Refusal;
            match crate::admission::identify(
                &identity.app,
                &identity.headers,
                Some(identity.authenticated.clone()),
                &identity.policy,
            )
            .await
            {
                Ok(_) => {},
                Err(Refusal::IdentityUnavailable) => {
                    return Err(StreamRefusal::Unavailable(EnrichmentOutcome::UNAVAILABLE_MESSAGE));
                },
                Err(_) => return Err(StreamRefusal::Forbidden(EnrichmentOutcome::DENIED_MESSAGE)),
            }
        }
        Ok(())
    }
}
