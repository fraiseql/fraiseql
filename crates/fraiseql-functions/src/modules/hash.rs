//! A function's integrity hash: its own source and every shared module it imports.
//!
//! SHA-256 over one encoding, in this order:
//!
//! 1. the domain tag [`DOMAIN`], which carries the encoding's version;
//! 2. the entry module's source;
//! 3. each shared module's key and source, in key order.
//!
//! Every field is length-prefixed (a `u64`, little-endian), so the encoding is
//! injective: no two different module sets produce the same bytes, however their
//! keys and contents are split. Key order makes the hash independent of the order
//! imports were found in. It covers the **source** each module was read as, not the
//! `JavaScript` it is transpiled to, so it names what the author wrote.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::resolve::ModuleKey;

/// The domain tag: what is hashed, and the encoding's version.
pub const DOMAIN: &str = "fraiseql.function-module.v1";

/// The hex SHA-256 of `entry` and the shared `modules` it imports.
#[must_use]
pub fn function_hash<S: AsRef<[u8]>>(entry: &[u8], modules: &BTreeMap<ModuleKey, S>) -> String {
    let mut hasher = Sha256::new();
    field(&mut hasher, DOMAIN.as_bytes());
    field(&mut hasher, entry);
    for (key, source) in modules {
        field(&mut hasher, key.as_str().as_bytes());
        field(&mut hasher, source.as_ref());
    }
    hex::encode(hasher.finalize())
}

/// One length-prefixed field.
fn field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
