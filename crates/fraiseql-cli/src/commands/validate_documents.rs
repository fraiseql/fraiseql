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

/// Validation result for a single document entry.
struct EntryResult {
    key:   String,
    valid: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    #[allow(dead_code)]
    version:   u32,
    documents: HashMap<String, String>,
}

/// Run the `validate-documents` command.
pub fn run(manifest_path: &str) -> Result<bool> {
    let path = Path::new(manifest_path);
    let contents = std::fs::read_to_string(path)
        .context(format!("Failed to read manifest: {manifest_path}"))?;

    let manifest: Manifest = serde_json::from_str(&contents)
        .context(format!("Failed to parse manifest JSON: {manifest_path}"))?;

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
    println!("Trusted documents manifest: {manifest_path}");
    println!("Total documents: {total}");
    println!("Valid: {valid_count}");

    if error_count > 0 {
        println!("Errors: {error_count}");
        println!();
        for r in &results {
            if let Some(ref err) = r.error {
                println!("  {} — {err}", r.key);
            }
        }
        Ok(false)
    } else {
        println!("All documents valid.");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let result = run(path.to_str().unwrap()).unwrap();
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

        let result = run(path.to_str().unwrap()).unwrap();
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

        let result = run(path.to_str().unwrap()).unwrap();
        assert!(!result);
    }
}
