//! `fraiseql validate-documents` — validate a trusted documents manifest.
//!
//! Checks:
//! 1. The manifest JSON is well-formed
//! 2. Each key is a valid SHA-256 hex string matching its query body
//! 3. Exits 0 on success, 2 on validation failure

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::output::OutputFormatter;

/// Validation result for a single document entry.
struct EntryResult {
    key:   String,
    valid: bool,
    error: Option<String>,
}

const SUPPORTED_MANIFEST_VERSION: u32 = 1;

/// Maximum manifest file size accepted (10 MiB).
///
/// Manifests larger than this limit are rejected before reading into memory to
/// prevent trivial OOM attacks via a crafted large file.
const MAX_MANIFEST_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    version:   u32,
    documents: HashMap<String, String>,
}

/// Run the `validate-documents` command.
pub fn run(manifest_path: &str, formatter: &OutputFormatter) -> Result<bool> {
    let path = Path::new(manifest_path);

    // Reject oversized files before reading into memory.
    let metadata = std::fs::metadata(path)
        .context(format!("Failed to read manifest: {manifest_path}"))?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        anyhow::bail!(
            "Manifest file {manifest_path} is too large ({} bytes); \
             the maximum accepted size is {} bytes (10 MiB)",
            metadata.len(),
            MAX_MANIFEST_BYTES,
        );
    }

    let contents = std::fs::read_to_string(path)
        .context(format!("Failed to read manifest: {manifest_path}"))?;

    let manifest: Manifest = serde_json::from_str(&contents)
        .context(format!("Failed to parse manifest JSON: {manifest_path}"))?;

    if manifest.version != SUPPORTED_MANIFEST_VERSION {
        anyhow::bail!(
            "Unsupported manifest version {}; this version of fraiseql-cli supports version {}",
            manifest.version,
            SUPPORTED_MANIFEST_VERSION,
        );
    }

    let total = manifest.documents.len();
    let mut results: Vec<EntryResult> = Vec::with_capacity(total);

    for (key, body) in &manifest.documents {
        let hash_hex = key.strip_prefix("sha256:").unwrap_or(key);

        // Validate hex string length (SHA-256 = 64 hex chars)
        if hash_hex.len() != 64 || !hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            results.push(EntryResult {
                key:   key.clone(),
                valid: false,
                error: Some(format!(
                    "Invalid SHA-256 hash: expected 64 hex characters, got {} chars",
                    hash_hex.len()
                )),
            });
            continue;
        }

        // Compute SHA-256 of the query body and compare
        let computed = format!("{:x}", Sha256::digest(body.as_bytes()));
        if computed == hash_hex {
            results.push(EntryResult {
                key:   key.clone(),
                valid: true,
                error: None,
            });
        } else {
            results.push(EntryResult {
                key:   key.clone(),
                valid: false,
                error: Some(format!("Hash mismatch: computed {computed}")),
            });
        }
    }

    let valid_count = results.iter().filter(|r| r.valid).count();
    let error_count = results.iter().filter(|r| !r.valid).count();

    // Print summary
    formatter.progress(&format!("Trusted documents manifest: {manifest_path}"));
    formatter.progress(&format!("Total documents: {total}"));
    formatter.progress(&format!("Valid: {valid_count}"));

    if error_count > 0 {
        formatter.progress(&format!("Errors: {error_count}"));
        formatter.progress("");
        for r in &results {
            if let Some(ref err) = r.error {
                formatter.progress(&format!("  {} - {err}", r.key));
            }
        }
        Ok(false)
    } else {
        formatter.progress("All documents valid.");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::*;
    use crate::output::OutputFormatter;

    #[test]
    fn test_rejects_manifest_exceeding_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.json");

        // Write a file of MAX_MANIFEST_BYTES + 1 bytes (just over the limit).
        let size = usize::try_from(MAX_MANIFEST_BYTES).unwrap() + 1;
        std::fs::write(&path, vec![b'x'; size]).unwrap();

        let formatter = OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too large"), "expected size error, got: {msg}");
    }

    #[test]
    fn test_rejects_unknown_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = serde_json::json!({
            "version": 99,
            "documents": {}
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unsupported manifest version"), "expected version error, got: {msg}");
    }

    #[test]
    fn valid_manifest_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let query = "{ users { id } }";
        let hash = format!("{:x}", Sha256::digest(query.as_bytes()));
        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                format!("sha256:{hash}"): query
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter).unwrap();
        assert!(result);
    }

    #[test]
    fn mismatched_hash_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                "sha256:0000000000000000000000000000000000000000000000000000000000000000": "{ users { id } }"
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter).unwrap();
        assert!(!result);
    }

    #[test]
    fn invalid_hash_length_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                "sha256:tooshort": "{ users { id } }"
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter).unwrap();
        assert!(!result);
    }
}
