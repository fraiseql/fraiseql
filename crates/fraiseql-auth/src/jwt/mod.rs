//! JWT claims: the session claims the server mints, and the readers that flatten
//! nested provider claims.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{AuthError, Result};

/// Maximum age of a JWT token measured from its `iat` (issued-at) claim.
///
/// Tokens whose `iat` is more than this many seconds in the past are rejected
/// as potentially replayed credentials.  24 h is a conservative upper bound —
/// short-lived access tokens expire via `exp` long before this limit is reached;
/// this guard targets long-lived or replayed tokens that somehow passed `exp` checks.
pub const MAX_TOKEN_AGE_SECS: u64 = 86_400;

/// Maximum allowed clock skew for `iat` and `nbf` claim checks.
///
/// A 5-minute window accommodates minor time drift between issuer and validator
/// without opening a meaningful forgery window.
pub const MAX_CLOCK_SKEW_SECS: u64 = 300;

/// Standard JWT claims with support for custom claims
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    /// Subject (typically user ID)
    pub sub:   String,
    /// Issued at (Unix timestamp)
    pub iat:   u64,
    /// Expiration time (Unix timestamp)
    pub exp:   u64,
    /// Not-before time (Unix timestamp) — optional per RFC 7519 §4.1.5.
    ///
    /// When present, the token MUST NOT be accepted before this time (plus
    /// [`MAX_CLOCK_SKEW_SECS`]).  When absent, the not-before check is skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf:   Option<u64>,
    /// Issuer
    pub iss:   String,
    /// Audience
    pub aud:   Vec<String>,
    /// Additional custom claims
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Claims {
    /// Get a custom claim by name
    #[must_use]
    pub fn get_custom(&self, key: &str) -> Option<&serde_json::Value> {
        self.extra.get(key)
    }

    /// Extract the `email` claim as a flat string.
    ///
    /// Handles plain strings, nested objects (`{"value": "..."}`,
    /// `{"email": "..."}`), and arrays (first string element).
    /// Returns `None` when the claim is absent, null, or cannot be
    /// normalised to a non-empty string.
    #[must_use]
    pub fn email(&self) -> Option<String> {
        self.extra.get("email").and_then(extract_claim_string)
    }

    /// Extract the `name` claim as a flat display-name string.
    ///
    /// In addition to the shapes handled by [`extract_claim_string`],
    /// this also concatenates `given` + `family` keys when the claim is
    /// an object without a `formatted` or `value` key.
    /// Returns `None` when the claim is absent or cannot be normalised.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.extra.get("name").and_then(extract_name_string)
    }

    /// Check if token is expired
    ///
    /// SECURITY: If system time cannot be determined, returns true (treats token as expired)
    /// This is a fail-safe approach to prevent accepting tokens when we can't verify expiry
    pub fn is_expired(&self) -> bool {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(e) => {
                // CRITICAL: System time failure - treat token as expired (fail-safe)
                // Log this critical error for operators to investigate
                tracing::error!(
                    error = %e,
                    "CRITICAL: System time error in token expiry check — \
                     this indicates a system clock issue. Token rejected as safety measure."
                );
                // Return current time as far in the future to ensure token is expired
                u64::MAX
            },
        };
        self.exp <= now
    }

    /// Validate temporal claims: `iat` staleness/skew and `nbf` not-before.
    ///
    /// Enforces three RFC 7519 temporal guards beyond `exp`:
    ///
    /// - `iat` must not be more than [`MAX_CLOCK_SKEW_SECS`] seconds in the future (forgery guard —
    ///   a future `iat` is implausible for a legitimately issued token).
    /// - `iat` must not be more than [`MAX_TOKEN_AGE_SECS`] seconds in the past (replay guard — a
    ///   stale `iat` indicates a replayed or abnormally long-lived token).
    /// - `nbf` (if present) must not be more than [`MAX_CLOCK_SKEW_SECS`] seconds in the future
    ///   (RFC 7519 §4.1.5 not-before enforcement).
    ///
    /// # Errors
    ///
    /// - [`AuthError::TokenIssuedInFuture`] if `iat > now + MAX_CLOCK_SKEW_SECS`.
    /// - [`AuthError::TokenTooOld`] if `now - iat > MAX_TOKEN_AGE_SECS`.
    /// - [`AuthError::TokenNotYetValid`] if `nbf > now + MAX_CLOCK_SKEW_SECS`.
    /// - [`AuthError::SystemTimeError`] if the system clock cannot be read.
    pub fn validate_temporal_claims(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AuthError::SystemTimeError {
                message: format!("Cannot determine current time for temporal validation: {e}"),
            })?
            .as_secs();

        // iat: must not be substantially in the future (forgery / clock-skew guard).
        if self.iat > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
            return Err(AuthError::TokenIssuedInFuture);
        }

        // iat: must not be older than MAX_TOKEN_AGE_SECS (replay guard).
        if now.saturating_sub(self.iat) > MAX_TOKEN_AGE_SECS {
            return Err(AuthError::TokenTooOld);
        }

        // nbf: not-before — token must not be used before the claim (with clock skew).
        if let Some(nbf) = self.nbf {
            if nbf > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
                return Err(AuthError::TokenNotYetValid);
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Nested claim extraction
// ---------------------------------------------------------------------------

/// Trim a string and return `None` if the result is empty.
fn trim_or_none(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Extract a flat string from a potentially nested JWT claim value.
///
/// Many identity providers (Azure AD, some OIDC providers) return structured
/// objects for standard claims like `email` and `name` instead of flat strings.
/// This function normalises those shapes into a single `Option<String>`.
///
/// Supported shapes:
/// - **String**: returned as-is (after trim; empty/whitespace → `None`).
/// - **Object**: tries keys `value`, `formatted`, `email` in order; falls back to the first string
///   value in the object.
/// - **Array**: returns the first element that is a string.
/// - **Null / number / bool**: returns `None`.
#[must_use]
pub fn extract_claim_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => trim_or_none(s),

        serde_json::Value::Object(map) => {
            // Priority order for well-known keys
            for key in &["value", "formatted", "email"] {
                if let Some(serde_json::Value::String(s)) = map.get(*key) {
                    if let Some(v) = trim_or_none(s) {
                        return Some(v);
                    }
                }
            }
            // Fallback: first string value in the object
            for v in map.values() {
                if let serde_json::Value::String(s) = v {
                    if let Some(v) = trim_or_none(s) {
                        return Some(v);
                    }
                }
            }
            None
        },

        serde_json::Value::Array(arr) => arr.iter().find_map(|v| {
            if let serde_json::Value::String(s) = v {
                trim_or_none(s)
            } else {
                None
            }
        }),

        _ => None,
    }
}

/// Extract a display name from a potentially nested JWT `name` claim.
///
/// Tries [`extract_claim_string`] first.  If that returns `None` and the value
/// is an object with `given` and/or `family` keys, concatenates them in Western
/// order (`"{given} {family}"`).  Empty/whitespace-only parts are dropped; if
/// both are empty the function returns `None`.
pub fn extract_name_string(value: &serde_json::Value) -> Option<String> {
    match value {
        // Strings and arrays: delegate to generic extraction.
        serde_json::Value::String(_) | serde_json::Value::Array(_) => extract_claim_string(value),

        // Objects: try well-known keys first, then given+family concatenation.
        serde_json::Value::Object(map) => {
            // Priority keys (same as extract_claim_string)
            for key in &["value", "formatted", "email"] {
                if let Some(serde_json::Value::String(s)) = map.get(*key) {
                    if let Some(v) = trim_or_none(s) {
                        return Some(v);
                    }
                }
            }

            // Name-specific: given + family concatenation.
            let given = map.get("given").and_then(|v| v.as_str()).and_then(trim_or_none);
            let family = map.get("family").and_then(|v| v.as_str()).and_then(trim_or_none);

            match (given, family) {
                (Some(g), Some(f)) => Some(format!("{g} {f}")),
                (Some(g), None) => Some(g),
                (None, Some(f)) => Some(f),
                (None, None) => None,
            }
        },

        _ => None,
    }
}

#[cfg(test)]
mod tests;
