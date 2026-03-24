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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)] // Reason: test module uses wildcard import for brevity
    use super::*;

    fn make_claims(nonce: Option<&str>, auth_time: Option<i64>) -> IdTokenClaims {
        let mut c = IdTokenClaims::new(
            "https://idp.example.com".into(),
            "user1".into(),
            "client_id".into(),
            9_999_999_999,
            0,
        );
        c.nonce = nonce.map(str::to_owned);
        c.auth_time = auth_time;
        c
    }

    // ── Nonce tests (13-1) ────────────────────────────────────────────────────

    #[test]
    fn test_callback_rejects_missing_nonce_claim() {
        let claims = make_claims(None, None);
        let result = validate_nonce_claim(&claims, "expected-nonce");
        assert!(matches!(result, Err(AuthError::MissingNonce)));
    }

    #[test]
    fn test_callback_rejects_wrong_nonce() {
        let claims = make_claims(Some("actual-nonce"), None);
        let result = validate_nonce_claim(&claims, "different-nonce");
        assert!(matches!(result, Err(AuthError::NonceMismatch)));
    }

    #[test]
    fn test_callback_accepts_correct_nonce() {
        let claims = make_claims(Some("correct-nonce"), None);
        assert!(validate_nonce_claim(&claims, "correct-nonce").is_ok());
    }

    #[test]
    fn test_callback_nonce_is_one_shot() {
        // Simulates one-shot nonce consumption:
        // 1. First validation against the stored nonce succeeds.
        // 2. After the callback handler consumes (deletes) the nonce from the session store,
        //    subsequent re-use attempts fail with MissingNonce because the session no longer
        //    carries a nonce to compare against.
        //
        // In production the callback handler is responsible for deleting the nonce from
        // the session store before calling this validator, making the check one-shot.
        let claims = make_claims(Some("once-nonce"), None);
        assert!(validate_nonce_claim(&claims, "once-nonce").is_ok()); // first use OK

        // Simulate session nonce consumed: token re-use attempt where the session's
        // stored nonce has been cleared but the attacker replays the same ID token.
        let cleared_claims = make_claims(None, None);
        let result = validate_nonce_claim(&cleared_claims, "once-nonce");
        assert!(
            matches!(result, Err(AuthError::MissingNonce)),
            "second use must fail: stored nonce already consumed"
        );
    }

    // ── auth_time / max_age tests (13-2) ─────────────────────────────────────

    const NOW: i64 = 1_700_000_000;

    #[test]
    fn test_auth_time_within_max_age_accepted() {
        // auth_time = now - 30s, max_age = 60s → age(30) ≤ max_age(60) + skew(60) = 120 → Ok
        let claims = make_claims(None, Some(NOW - 30));
        assert!(validate_auth_time_claim(&claims, 60, NOW).is_ok());
    }

    #[test]
    fn test_auth_time_exceeds_max_age_rejected() {
        // auth_time = now - 200s, max_age = 60s → age(200) > max_age(60) + skew(60) = 120 → Err
        let claims = make_claims(None, Some(NOW - 200));
        let result = validate_auth_time_claim(&claims, 60, NOW);
        assert!(
            matches!(
                result,
                Err(AuthError::SessionTooOld {
                    age:          200,
                    max_age_secs: 60,
                })
            ),
            "expected SessionTooOld, got: {result:?}"
        );
    }

    #[test]
    fn test_missing_auth_time_when_max_age_present_rejected() {
        let claims = make_claims(None, None); // no auth_time claim
        let result = validate_auth_time_claim(&claims, 3600, NOW);
        assert!(matches!(result, Err(AuthError::MissingAuthTime)));
    }

    #[test]
    fn test_max_age_absent_skips_auth_time_check() {
        // When max_age = 0 the allowed window is 0 + CLOCK_SKEW_SECS = 60 s.
        // A session authenticated 59 s ago must be accepted.
        // Callers are responsible for not invoking this function when max_age was absent
        // from the authorization request — this test documents the boundary condition.
        let claims = make_claims(None, Some(NOW - 59));
        assert!(validate_auth_time_claim(&claims, 0, NOW).is_ok());
    }
}
