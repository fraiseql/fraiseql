//! The identity a per-user rate-limit decision may be keyed on (#1171).
//!
//! #1143 deleted the HTTP per-user allowance, and the reason is the whole design of this
//! module. The only identity available at the limiter came from `extract_jwt_subject`,
//! which base64-decoded a JWT payload and never checked the signature. A fresh key is a
//! fresh *full* bucket, so varying `sub` did not merely grow the map: it handed the
//! caller an unlimited budget. Measured then: 50 of 50 requests allowed against
//! `rps_per_ip = 1, burst = 1`, from one IP, unauthenticated. Deleting it removed a
//! bypass rather than a control.
//!
//! What it left behind is a real gap, though: every HTTP request buckets on its IP, so
//! all the authenticated users behind one egress address share one budget — and the
//! per-user allowance exists precisely because an identified caller is a different risk
//! from an anonymous address.
//!
//! Restoring it needs a *verified* subject, which is what this seam carries: the
//! deployment's own configured validator, not a decoder. A token that does not verify
//! yields `None` and the request falls back to the IP bucket, so a forged JWT cannot
//! mint anything — the property #1143 established, kept as a test.
//!
//! The verification is a second one: the transport's auth layer runs later and does its
//! own, plus revocation and the service-account seam. Sharing a result across the two
//! would mean this layer deciding what counts as authenticated, which is not its job. An
//! HS256 check is an HMAC; an OIDC check is an RS256 verify against an already-cached
//! JWKS. Neither is a network call on the hot path.

use std::sync::Arc;

use axum::http::{HeaderMap, header};
use fraiseql_core::security::{AuthMiddleware, AuthRequest, OidcValidator};

/// The validator a per-user rate-limit decision verifies its subject with.
///
/// An enum rather than a trait object, because the set is closed and known: `attach_auth`
/// picks between exactly these two modes, in this order. Adding a third auth mode should
/// be a compile error here as well as there — a `dyn` seam would instead let the limiter
/// silently keep bucketing everyone on their address.
pub enum VerifiedSubject {
    /// OIDC: signature, issuer, audience and expiry, against the deployment's JWKS.
    Oidc(Arc<OidcValidator>),
    /// HS256: the shared-secret twin, using the validator the `[auth_hs256]` transport
    /// layer uses.
    Hs256(Arc<AuthMiddleware>),
}

impl VerifiedSubject {
    /// The verified `sub`, or `None`.
    ///
    /// `None` is the answer for every request that is not provably from a known caller:
    /// no credential, a malformed one, a bad signature, an expired token. Those bucket on
    /// the client address, which is what keeps #1143's fix intact.
    pub async fn subject(&self, headers: &HeaderMap) -> Option<String> {
        let token = bearer_token(headers)?;
        match self {
            Self::Oidc(validator) => validator
                .validate_token(&token)
                .await
                .ok()
                .map(|user| user.user_id.as_str().to_owned()),
            Self::Hs256(validator) => validator
                .validate_request(&AuthRequest::new(Some(format!("Bearer {token}"))))
                .ok()
                .map(|user| user.user_id.as_str().to_owned()),
        }
    }
}

/// The bearer token, from the `Authorization` header or the `__Host-access_token`
/// cookie — the same two places [`crate::middleware::oidc_auth_middleware`] looks, so a
/// request cannot be authenticated by the transport and anonymous to the limiter.
fn bearer_token(headers: &HeaderMap) -> Option<String> {
    match headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        Some(value) => value.strip_prefix("Bearer ").map(ToOwned::to_owned),
        None => crate::middleware::oidc_auth::extract_access_token_cookie(headers),
    }
}
