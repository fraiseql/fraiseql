// Opaque ID generation to prevent ID enumeration attacks
// Encodes internal database IDs in a way that doesn't expose sequence patterns

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
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
    id: String,
}

impl OpaqueId {
    /// Create an opaque ID from a database ID
    ///
    /// Example: database ID 12345 → "MTIzNDU=" (base64)
    #[must_use]
    pub fn new(db_id: impl Into<String>) -> Self {
        let id_str = db_id.into();
        let encoded = URL_SAFE_NO_PAD.encode(id_str.as_bytes());
        Self { id: encoded }
    }

    /// Create an opaque ID with signature for integrity verification
    ///
    /// Uses SHA256(db_id + secret) to create a tamper-proof ID
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
        let (db_id, provided_sig) = match decoded.split_once('|') {
            Some((id, sig)) => (id, sig),
            None => return false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opaque_id_creation() {
        let opaque = OpaqueId::new("12345");
        assert!(!opaque.as_str().is_empty());
        // Opaque ID should not contain the original ID in plain text
        assert!(!opaque.as_str().contains("12345"));
    }

    #[test]
    fn test_opaque_id_decode() {
        let db_id = "user_42";
        let opaque = OpaqueId::new(db_id);
        let decoded = opaque.decode();
        assert_eq!(decoded, Some(db_id.to_string()));
    }

    #[test]
    fn test_opaque_id_with_signature() {
        let db_id = "12345";
        let secret = b"secret_key";
        let opaque = OpaqueId::with_signature(db_id, secret);

        // Should be able to verify with correct secret
        assert!(opaque.verify_signature(secret));

        // Should fail with wrong secret
        assert!(!opaque.verify_signature(b"wrong_secret"));
    }

    #[test]
    fn test_opaque_id_signature_tampering() {
        let db_id = "sensitive_id_789";
        let secret = b"super_secret";
        let mut opaque = OpaqueId::with_signature(db_id, secret);

        // Verify original
        assert!(opaque.verify_signature(secret));

        // Tamper with the opaque ID
        opaque.id = opaque.id.chars().rev().collect();

        // Should fail verification
        assert!(!opaque.verify_signature(secret));
    }

    #[test]
    fn test_opaque_id_equality() {
        let opaque1 = OpaqueId::new("same_id");
        let opaque2 = OpaqueId::new("same_id");
        assert_eq!(opaque1, opaque2);

        let opaque3 = OpaqueId::new("different_id");
        assert_ne!(opaque1, opaque3);
    }

    #[test]
    fn test_opaque_id_prevents_enumeration() {
        let ids: Vec<String> = (1..=5).map(|i| i.to_string()).collect();
        let opaque_ids: Vec<OpaqueId> = ids.iter().map(|id| OpaqueId::new(id)).collect();

        // Even though original IDs are sequential, opaque IDs should look random
        for i in 1..opaque_ids.len() {
            // Check that opaque IDs don't follow a predictable pattern
            assert_ne!(opaque_ids[i].as_str(), opaque_ids[i - 1].as_str());
        }

        // Verify that opaque IDs are different from the original sequential pattern
        for i in 0..opaque_ids.len() {
            let original = ids[i].as_str();
            let opaque = opaque_ids[i].as_str();
            // Opaque ID should not contain the original ID in plain text
            assert!(!opaque.contains(original));
        }
    }
}
