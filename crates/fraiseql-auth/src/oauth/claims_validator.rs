//! Validation helpers for OIDC ID token claims.
//!
//! These functions operate on decoded [`IdTokenClaims`] and verify the
//! security properties required by OpenID Connect Core §3.1.3.7:
//! nonce replay protection and `auth_time`/`max_age` session-age enforcement.
//!
//! Both helpers are intentionally stateless so they can be unit-tested in
//! isolation from JWT parsing and JWKS key fetching.

use subtle::ConstantTimeEq as _;

use crate::{
    error::{AuthError, Result},
    oauth::types::IdTokenClaims,
};

/// Clock-skew tolerance for `auth_time` comparisons (OpenID Connect Core §3.1.3.7).
///
/// A 60-second allowance accommodates minor time drift between the relying party
/// and the identity provider without opening a meaningful replay window.
pub const CLOCK_SKEW_SECS: i64 = 60;

/// Verify the `nonce` claim in an ID token against the expected value.
///
/// Uses constant-time comparison to prevent timing oracles that could leak
/// information about the stored nonce value (RFC 6749 §10.12).
///
/// # Errors
///
/// - [`AuthError::MissingNonce`] — the token carries no `nonce` claim.
/// - [`AuthError::NonceMismatch`] — the token's nonce does not match `expected_nonce`.
pub fn validate_nonce_claim(claims: &IdTokenClaims, expected_nonce: &str) -> Result<()> {
    let token_nonce = claims.nonce.as_deref().ok_or(AuthError::MissingNonce)?;
    if token_nonce.as_bytes().ct_eq(expected_nonce.as_bytes()).into() {
        Ok(())
    } else {
        Err(AuthError::NonceMismatch)
    }
}

/// Verify the `auth_time` claim in an ID token against a `max_age` constraint.
///
/// Enforces OpenID Connect Core §3.1.3.7: if `max_age` was included in the
/// authorization request, the ID token MUST contain an `auth_time` claim and
/// `now - auth_time ≤ max_age + CLOCK_SKEW_SECS` must hold.
///
/// # Arguments
///
/// - `claims` — the decoded ID token claims.
/// - `max_age_secs` — the `max_age` value sent in the authorization request.
/// - `now_secs` — current Unix timestamp (injectable for deterministic testing).
///
/// # Errors
///
/// - [`AuthError::MissingAuthTime`] — the token carries no `auth_time` claim.
/// - [`AuthError::SessionTooOld`] — the session was authenticated more than `max_age_secs +
///   CLOCK_SKEW_SECS` seconds ago.
pub fn validate_auth_time_claim(
    claims: &IdTokenClaims,
    max_age_secs: u64,
    now_secs: i64,
) -> Result<()> {
    let auth_time = claims.auth_time.ok_or(AuthError::MissingAuthTime)?;
    let age = now_secs.saturating_sub(auth_time);
    let max_age_i64 = i64::try_from(max_age_secs).unwrap_or(i64::MAX);
    let allowed = max_age_i64.saturating_add(CLOCK_SKEW_SECS);
    if age > allowed {
        Err(AuthError::SessionTooOld { age, max_age_secs })
    } else {
        Ok(())
    }
}

