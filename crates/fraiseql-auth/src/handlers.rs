//! Helpers the authentication routes share: the random `state` a login flow stores, and the
//! length guard on tokens a client submits.

/// Generate a cryptographically random state for CSRF protection.
///
/// Uses `rand::rng()` (a reseeding thread-local CSPRNG, ChaCha-based) — a
/// cryptographically secure source. The doc previously claimed `OsRng`, which
/// this does not call; corrected rather than switched, since `rand::rng()` is
/// itself a CSPRNG and the entropy guarantee holds (#737).
#[must_use]
pub fn generate_secure_state() -> String {
    use rand::RngCore as _;

    // Generate 32 random bytes for 256 bits of entropy
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);

    // Encode as hex string for safe transmission in URLs/headers
    hex::encode(bytes)
}

/// Maximum byte length for a refresh token submitted to `/auth/v1/session/{refresh,logout}`.
///
/// Session tokens are usually ≤ 2 KiB; 4 KiB covers all real-world formats
/// while bounding memory allocation for an attacker-supplied value.
pub const MAX_REFRESH_TOKEN_BYTES: usize = 4_096;

/// Guard oversized auth inputs before they reach session-store or provider logic.
///
/// Returns `AuthError::InvalidToken` with an internal reason string when
/// `input` exceeds `max_bytes`.  The reason is logged server-side and never
/// forwarded to the client.
///
/// # Errors
///
/// Returns `AuthError::InvalidToken` when `input.len() > max_bytes`.
pub fn validate_auth_input_len(
    input: &str,
    max_bytes: usize,
    field: &str,
) -> crate::error::Result<()> {
    if input.len() > max_bytes {
        return Err(crate::error::AuthError::InvalidToken {
            reason: format!("{field} exceeds maximum length ({} > {max_bytes} bytes)", input.len()),
        });
    }
    Ok(())
}
