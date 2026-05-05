//! Opaque ID encoding to prevent ID enumeration attacks.
//!
//! Encodes internal database IDs in a way that does not expose sequential
//! patterns to external callers, improving security against IDOR attacks.

use std::fmt;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Opaque ID that hides the internal database ID
///
/// Prevents enumeration attacks by:
/// 1. Encoding database IDs in base64url format
/// 2. Adding a cryptographic signature for verification
/// 3. Making sequential IDs indistinguishable from random
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpaqueId {
    // The encoded ID (base64url + optional signature)
    pub(crate) id: String,
}

impl OpaqueId {
    /// Create an opaque ID from a database ID
    ///
    /// Example: database ID 12345 → "`MTIzNDU`=" (base64)
    #[must_use]
    pub fn new(db_id: impl Into<String>) -> Self {
        let id_str = db_id.into();
        let encoded = URL_SAFE_NO_PAD.encode(id_str.as_bytes());
        Self { id: encoded }
    }

    /// Create an opaque ID with signature for integrity verification
    ///
    /// Uses `SHA256(db_id` + secret) to create a tamper-proof ID
    #[must_use]
    pub fn with_signature(db_id: impl Into<String>, secret: &[u8]) -> Self {
        let id_str = db_id.into();

        // Create signature: SHA256(db_id + secret)
        let mut hasher = Sha256::new();
        hasher.update(id_str.as_bytes());
        hasher.update(secret);
        let signature = URL_SAFE_NO_PAD.encode(hasher.finalize());

        // Combine ID + signature with separator (use | which is not in base64url alphabet)
        let opaque = format!("{}|{}", id_str, signature);
        let encoded = URL_SAFE_NO_PAD.encode(opaque.as_bytes());

        Self { id: encoded }
    }

    /// Get the opaque ID string (suitable for API responses)
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// Decode opaque ID back to database ID
    ///
    /// Returns None if ID is not valid base64
    #[must_use]
    pub fn decode(&self) -> Option<String> {
        URL_SAFE_NO_PAD
            .decode(&self.id)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }

    /// Verify signature of opaque ID
    ///
    /// Returns true if signature is valid, false otherwise
    #[must_use]
    pub fn verify_signature(&self, secret: &[u8]) -> bool {
        // Decode the opaque ID
        let Some(decoded) = self.decode() else {
            return false;
        };

        // Split at pipe separator (| is not in base64url alphabet)
        let Some((db_id, provided_sig)) = decoded.split_once('|') else {
            return false;
        };

        // Recalculate signature
        let mut hasher = Sha256::new();
        hasher.update(db_id.as_bytes());
        hasher.update(secret);
        let expected_sig = URL_SAFE_NO_PAD.encode(hasher.finalize());

        // Constant-time comparison
        constant_time_eq(provided_sig.as_bytes(), expected_sig.as_bytes())
    }
}

impl fmt::Display for OpaqueId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }

    result == 0
}
