//! The selector + verifier opaque-token codec shared by the local-password flows.
//!
//! Both password reset (#367) and email verification (#945) hand the user a
//! `b64url(16B selector) "." b64url(32B verifier)` string and persist only the
//! selector (non-secret, indexed) plus `sha256(verifier)`. The security argument is
//! identical for both, so the codec lives here once rather than being reimplemented
//! per flow:
//!
//! - Redemption fetches the row `WHERE selector = $1` — no secret in the `WHERE`, so the lookup is
//!   not an existence oracle — then compares `sha256(presented verifier)` against the stored hash
//!   in constant time.
//! - A full database read cannot forge a usable token: that needs a SHA-256 preimage of a 256-bit
//!   CSPRNG verifier. SHA-256 rather than Argon2 is sufficient precisely because the verifier is
//!   high-entropy — there is no brute-force surface a KDF's cost would defend.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

use crate::error::{AuthError, Result};

/// Selector length in bytes (the non-secret, indexed lookup key).
const SELECTOR_LEN: usize = 16;
/// Verifier length in bytes (the secret; only its SHA-256 is stored).
const VERIFIER_LEN: usize = 32;

/// A freshly generated opaque token: a non-secret selector plus a secret verifier.
pub(super) struct OpaqueToken {
    selector: [u8; SELECTOR_LEN],
    verifier: [u8; VERIFIER_LEN],
}

/// The redemption-relevant parts of a presented token: the selector (for lookup) and
/// the SHA-256 of the verifier (for constant-time comparison against the stored hash).
pub(super) struct ParsedToken {
    pub(super) selector:      String,
    pub(super) verifier_hash: Vec<u8>,
}

impl OpaqueToken {
    /// Generate a token from the OS-seeded CSPRNG ([`rand::rng`], as used for refresh
    /// tokens).
    pub(super) fn generate() -> Self {
        use rand::RngCore as _;
        let mut selector = [0u8; SELECTOR_LEN];
        let mut verifier = [0u8; VERIFIER_LEN];
        rand::rng().fill_bytes(&mut selector);
        rand::rng().fill_bytes(&mut verifier);
        Self { selector, verifier }
    }

    /// The base64url selector, stored as the indexed lookup key.
    pub(super) fn selector_b64(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.selector)
    }

    /// SHA-256 of the verifier, the only verifier-derived value persisted.
    pub(super) fn verifier_hash(&self) -> Vec<u8> {
        Sha256::digest(self.verifier).to_vec()
    }

    /// The opaque token string handed to the user: `selector "." verifier`.
    pub(super) fn to_token_string(&self) -> String {
        format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(self.selector),
            URL_SAFE_NO_PAD.encode(self.verifier)
        )
    }

    /// Parse a presented token into its lookup selector and verifier hash.
    ///
    /// `kind` names the flow (`"reset"`, `"email verification"`) so the diagnostic
    /// reason reads correctly; callers map every failure to one generic client-facing
    /// error regardless.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if the token is not `selector.verifier`,
    /// either half is not valid base64url, or either decodes to the wrong length.
    pub(super) fn parse(kind: &str, token: &str) -> Result<ParsedToken> {
        let (selector_b64, verifier_b64) =
            token.split_once('.').ok_or_else(|| AuthError::InvalidToken {
                reason: format!("{kind} token is not in selector.verifier form"),
            })?;
        let selector =
            URL_SAFE_NO_PAD.decode(selector_b64).map_err(|_| AuthError::InvalidToken {
                reason: format!("{kind} token selector is not valid base64url"),
            })?;
        let verifier =
            URL_SAFE_NO_PAD.decode(verifier_b64).map_err(|_| AuthError::InvalidToken {
                reason: format!("{kind} token verifier is not valid base64url"),
            })?;
        if selector.len() != SELECTOR_LEN || verifier.len() != VERIFIER_LEN {
            return Err(AuthError::InvalidToken {
                reason: format!("{kind} token has an unexpected length"),
            });
        }
        Ok(ParsedToken {
            selector:      selector_b64.to_string(),
            verifier_hash: Sha256::digest(&verifier).to_vec(),
        })
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
