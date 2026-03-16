//! HMAC-chained tamper-evident audit log.
//!
//! Implements a cryptographic chain where each audit entry's `chain_hash` is
//! `HMAC-SHA256(prev_chain_hash || entry_json)`. This makes every entry depend
//! on all previous entries: retroactive modification, deletion, or insertion
//! of any single entry breaks the chain from that point forward.
//!
//! # Chain verification
//!
//! The [`verify_chain`] function performs a streaming O(n) pass over a sequence
//! of serialized entries, recomputing each hash and comparing it to the stored
//! `chain_hash` field. Any mismatch is reported as a [`ChainVerifyError`] with
//! the index of the first broken link.
//!
//! # Key management
//!
//! The chain seed (initial HMAC key) must be 32 bytes. In production it should
//! be stored in Vault or read from an environment variable (never in plaintext
//! config). Configure via `fraiseql.toml`:
//!
//! ```toml
//! [fraiseql.security.audit_logging]
//! tamper_evident = true
//! chain_seed_env = "AUDIT_CHAIN_SEED"  # 32-byte hex-encoded value
//! ```

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Core hashing
// ============================================================================

/// Compute an HMAC-SHA256 chain link.
///
/// The hash is `HMAC-SHA256(prev_hash || entry_json)` where `prev_hash` serves
/// as the HMAC key. This ensures each hash depends on the entire prior chain.
///
/// # Returns
///
/// A 32-byte raw HMAC-SHA256 output.
fn compute_chain_hash(prev_hash: &[u8; 32], entry_json: &str) -> [u8; 32] {
    // HMAC accepts any key length, so this expect is infallible.
    #[allow(clippy::unwrap_used)]
    // Reason: HmacSha256::new_from_slice accepts any key length (see HMAC spec §3),
    // so this can only fail if the key is empty. We pass a 32-byte array, so it
    // is always valid.
    let mut mac = HmacSha256::new_from_slice(prev_hash)
        .expect("HMAC-SHA256 accepts any non-empty key length");
    mac.update(entry_json.as_bytes());
    mac.finalize().into_bytes().into()
}

/// Hex-encode a 32-byte hash to a 64-character lowercase hex string.
fn encode_chain_hash(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

// ============================================================================
// ChainHasher — stateful hasher for sequential entry writing
// ============================================================================

/// Stateful HMAC chain hasher for sequential audit entry writing.
///
/// Maintains the running chain state. Call [`ChainHasher::advance`] for each
/// entry in order to produce the `chain_hash` value to embed in that entry.
///
/// # Thread safety
///
/// `ChainHasher` is **not** `Sync` by itself. For shared concurrent use, wrap
/// in a `Mutex<ChainHasher>` or `Arc<Mutex<ChainHasher>>`.
pub struct ChainHasher {
    current: [u8; 32],
}

impl ChainHasher {
    /// Create a new `ChainHasher` starting from the given seed.
    ///
    /// The seed must be exactly 32 bytes (256 bits). In production, derive it
    /// from Vault or an environment variable — never hardcode it.
    #[must_use]
    pub const fn new(seed: [u8; 32]) -> Self {
        Self { current: seed }
    }

    /// Advance the chain by one entry and return the hex-encoded chain hash.
    ///
    /// The returned value is the `chain_hash` field to embed in the entry.
    /// The internal state is updated so the next call depends on this hash.
    pub fn advance(&mut self, entry_json: &str) -> String {
        self.current = compute_chain_hash(&self.current, entry_json);
        encode_chain_hash(&self.current)
    }

    /// Return the current chain hash (hex-encoded) without advancing.
    #[must_use]
    pub fn current_hash(&self) -> String {
        encode_chain_hash(&self.current)
    }
}

// ============================================================================
// Chain verification
// ============================================================================

/// Error returned by [`verify_chain`] when the chain is broken.
#[derive(Debug, thiserror::Error)]
pub enum ChainVerifyError {
    /// A computed hash does not match the stored `chain_hash` at this entry.
    #[error(
        "Chain broken at entry {entry_index}: expected {expected_hash}, got {stored_hash}"
    )]
    BrokenLink {
        /// Zero-based index of the first broken entry.
        entry_index: usize,
        /// The hash we computed from the chain.
        expected_hash: String,
        /// The `chain_hash` stored in the entry.
        stored_hash: String,
    },
    /// An entry could not be parsed (missing or invalid `chain_hash` field).
    #[error("Entry {entry_index} is missing or has an invalid `chain_hash` field")]
    MissingChainHash {
        /// Zero-based index of the malformed entry.
        entry_index: usize,
    },
    /// An audit entry is not a JSON object.
    ///
    /// Every entry produced by the audit logger is a JSON object. If this
    /// variant is returned the supplied entries are malformed or have been
    /// tampered with.
    #[error("Entry {entry_index} is not a JSON object")]
    InvalidEntry {
        /// Zero-based index of the malformed entry.
        entry_index: usize,
    },
}

/// Verify the HMAC chain over a sequence of JSON entries.
///
/// Each entry must be a `serde_json::Value` with a `"chain_hash"` field containing
/// the hex-encoded 64-character hash. The entry JSON used for re-hashing is the
/// entry **without** the `chain_hash` field (i.e. the hash is computed over the
/// rest of the entry, then the result is stored in `chain_hash`).
///
/// Returns `Ok(count)` (total entries verified) if the chain is intact,
/// or `Err(ChainVerifyError)` at the first broken link.
///
/// # Memory
///
/// O(1) — processes entries one at a time without accumulating state.
///
/// # Errors
///
/// Returns [`ChainVerifyError::BrokenLink`] if any hash mismatches.
/// Returns [`ChainVerifyError::MissingChainHash`] if any entry lacks the field.
pub fn verify_chain(
    entries: impl IntoIterator<Item = serde_json::Value>,
    seed: [u8; 32],
) -> Result<usize, ChainVerifyError> {
    let mut hasher = ChainHasher::new(seed);
    let mut count = 0usize;

    for (idx, entry) in entries.into_iter().enumerate() {
        // Extract the stored chain_hash.
        let stored_hash = entry
            .get("chain_hash")
            .and_then(|v| v.as_str())
            .ok_or(ChainVerifyError::MissingChainHash { entry_index: idx })?
            .to_string();

        // Re-compute the hash over the entry without the chain_hash field.
        let mut entry_for_hash = entry;
        entry_for_hash
            .as_object_mut()
            .ok_or(ChainVerifyError::InvalidEntry { entry_index: idx })?
            .remove("chain_hash");
        let entry_json = entry_for_hash.to_string();
        let expected_hash = hasher.advance(&entry_json);

        if expected_hash != stored_hash {
            return Err(ChainVerifyError::BrokenLink {
                entry_index: idx,
                expected_hash,
                stored_hash,
            });
        }

        count += 1;
    }

    Ok(count)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    const TEST_SEED: [u8; 32] = *b"test-seed-32-bytes-exactly-here!";

    fn make_entry(action: &str, hasher: &mut ChainHasher) -> serde_json::Value {
        let mut entry = serde_json::json!({ "action": action, "user": "u1" });
        let hash = hasher.advance(&entry.to_string());
        entry["chain_hash"] = serde_json::Value::String(hash);
        entry
    }

    fn generate_chained_entries(n: usize, seed: [u8; 32]) -> Vec<serde_json::Value> {
        let mut hasher = ChainHasher::new(seed);
        (0..n)
            .map(|i| make_entry(&format!("action-{i}"), &mut hasher))
            .collect()
    }

    #[test]
    fn test_chain_hash_is_deterministic() {
        let h1 = compute_chain_hash(&TEST_SEED, "entry-1");
        let h2 = compute_chain_hash(&TEST_SEED, "entry-1");
        assert_eq!(h1, h2, "same inputs must produce same hash");
    }

    #[test]
    fn test_chain_hash_changes_with_content() {
        let h1 = compute_chain_hash(&TEST_SEED, r#"{"action":"query"}"#);
        let h2 = compute_chain_hash(&TEST_SEED, r#"{"action":"mutation"}"#);
        assert_ne!(h1, h2, "different content must produce different hash");
    }

    #[test]
    fn test_chain_is_sequential() {
        let h1 = compute_chain_hash(&TEST_SEED, "entry-1");
        let h2 = compute_chain_hash(&h1, "entry-2");
        let h3 = compute_chain_hash(&h2, "entry-3");
        // Re-compute h3 skipping h2 — must differ.
        let h3_alt = compute_chain_hash(&h1, "entry-3");
        assert_ne!(h3, h3_alt, "sequential hashes must differ from skipped chain");
    }

    #[test]
    fn test_chain_hash_output_is_64_hex_chars() {
        let h = encode_chain_hash(&compute_chain_hash(&TEST_SEED, "entry"));
        assert_eq!(h.len(), 64, "hex-encoded SHA256 must be 64 characters");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hasher_advance_changes_state() {
        let mut hasher = ChainHasher::new(TEST_SEED);
        let h1 = hasher.advance("entry-1");
        let h2 = hasher.advance("entry-1"); // same content, different state
        assert_ne!(h1, h2, "advancing changes internal state");
    }

    #[test]
    fn test_verify_valid_chain_passes() {
        let entries = generate_chained_entries(100, TEST_SEED);
        let result = verify_chain(entries, TEST_SEED);
        assert!(result.is_ok(), "valid chain must pass verification");
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_verify_detects_modified_entry() {
        let mut entries = generate_chained_entries(100, TEST_SEED);
        entries[50]["action"] = serde_json::Value::String("TAMPERED".to_string());
        let result = verify_chain(entries, TEST_SEED);
        assert!(
            matches!(result, Err(ChainVerifyError::BrokenLink { entry_index: 50, .. })),
            "modified entry must break chain at that index"
        );
    }

    #[test]
    fn test_verify_detects_deleted_entry() {
        let mut entries = generate_chained_entries(100, TEST_SEED);
        entries.remove(50);
        let result = verify_chain(entries, TEST_SEED);
        assert!(
            matches!(result, Err(ChainVerifyError::BrokenLink { entry_index: 50, .. })),
            "deleted entry must break chain at the deletion point"
        );
    }

    #[test]
    fn test_verify_empty_chain_passes() {
        let result = verify_chain([], TEST_SEED);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_verify_detects_missing_chain_hash() {
        let entries = vec![serde_json::json!({ "action": "query" })]; // no chain_hash
        let result = verify_chain(entries, TEST_SEED);
        assert!(
            matches!(result, Err(ChainVerifyError::MissingChainHash { entry_index: 0 }))
        );
    }
}
