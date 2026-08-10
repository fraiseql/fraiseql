//! Resolving *who is calling* a first-party auth route from its bearer token (#945).
//!
//! Sessions minted by [`PostgresSessionStore`](crate::PostgresSessionStore) carry the
//! account's `user_id` in the access token's `sub`. Routes that act **on the caller's own
//! account** — as email verification does — need that subject, and must not accept it
//! from the request body: a handler that trusts a body-supplied `user_id` is not
//! authenticated at all, it is a parameter.
//!
//! [`SessionBearerAuthenticator`] is that one seam. It validates the `Authorization:
//! Bearer` header against the same HS256 secret, issuer and audience the session store
//! signs with, and returns the subject — nothing else. It deliberately does not build a
//! richer principal: the routes that use it authorize nothing beyond "this is the account
//! the token was minted for".

use axum::http::{HeaderMap, header};
use jsonwebtoken::Algorithm;
use zeroize::Zeroizing;

use crate::{
    error::{AuthError, Result},
    jwt::JwtValidator,
};

/// Validates session access tokens and extracts the account they were minted for.
///
/// Construct it with the same `(secret, issuer, audience)` triple handed to
/// [`PostgresSessionStore::with_hs256_secret`](crate::PostgresSessionStore::with_hs256_secret)
/// and [`with_token_claims`](crate::PostgresSessionStore::with_token_claims) — a mismatch
/// means every call fails closed with [`AuthError::InvalidToken`], never opens.
pub struct SessionBearerAuthenticator {
    validator: JwtValidator,
    /// The HS256 signing secret, zeroized on drop.
    secret:    Zeroizing<Vec<u8>>,
}

/// Hand-written rather than derived: the struct holds a signing secret, and a derived
/// `Debug` would print it into any log line that formats the value.
impl std::fmt::Debug for SessionBearerAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionBearerAuthenticator").finish_non_exhaustive()
    }
}

impl SessionBearerAuthenticator {
    /// Build an authenticator pinned to one issuer and audience.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if `issuer` or `audience` is empty — an
    /// unpinned validator would accept a token minted for any service (the cross-service
    /// replay shape #359 closed for the server's own validator).
    pub fn new(secret: Vec<u8>, issuer: &str, audience: &str) -> Result<Self> {
        if audience.is_empty() {
            return Err(AuthError::ConfigError {
                message: "SessionBearerAuthenticator requires a non-empty audience: an unpinned \
                          validator accepts tokens minted for any service"
                    .to_string(),
            });
        }
        let validator = JwtValidator::new(issuer, Algorithm::HS256)?.with_audiences(&[audience])?;
        Ok(Self {
            validator,
            secret: Zeroizing::new(secret),
        })
    }

    /// Return the `user_id` the request's bearer token was minted for.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] when the `Authorization` header is absent,
    /// not a `Bearer` credential, or carries an empty subject; and whatever
    /// [`JwtValidator::validate_hmac`] returns (expired, bad signature, wrong issuer or
    /// audience) otherwise.
    pub fn subject(&self, headers: &HeaderMap) -> Result<String> {
        let raw =
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| AuthError::InvalidToken {
                    reason: "missing Authorization header".to_string(),
                })?;
        // Scheme match is ASCII-case-insensitive per RFC 7235; the credential itself is not.
        let token = raw
            .split_once(' ')
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, token)| token.trim())
            .ok_or_else(|| AuthError::InvalidToken {
                reason: "Authorization header is not a Bearer credential".to_string(),
            })?;

        let claims = self.validator.validate_hmac(token, &self.secret)?;
        if claims.sub.is_empty() {
            return Err(AuthError::InvalidToken {
                reason: "session token carries an empty subject".to_string(),
            });
        }
        Ok(claims.sub)
    }
}
