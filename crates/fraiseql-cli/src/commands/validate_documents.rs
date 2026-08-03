//! `fraiseql validate-documents` — validate a trusted documents manifest.
//!
//! Checks:
//! 1. The manifest JSON is well-formed
//! 2. Each key is a valid SHA-256 hex string matching its query body
//! 3. Exits 0 on success, 2 on validation failure

use std::{collections::HashMap, path::Path};

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
pub(crate) const MAX_MANIFEST_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    version:   u32,
    documents: HashMap<String, String>,
}

/// Run the `validate-documents` command.
///
/// With `max_cost` set, each hash-valid document is additionally scored with
/// [`fraiseql_core::graphql::estimate_query_cost`] (#379) — using the compiled
/// schema's `operation_cost_weights` when `schema_path` is given — and a
/// document over the ceiling fails validation. Scoring passes no variables, so
/// a variable-valued pagination argument costs its fail-closed ceiling: the
/// registered document's cost is its worst case.
///
/// # Errors
///
/// Returns an error if the manifest file cannot be read, exceeds the 10 MiB size
/// limit, cannot be parsed as JSON, specifies an unsupported manifest version,
/// or `schema_path` cannot be read/parsed.
pub fn run(
    manifest_path: &str,
    max_cost: Option<u64>,
    schema_path: Option<&str>,
    formatter: &OutputFormatter,
) -> Result<bool> {
    let path = Path::new(manifest_path);

    // Reject oversized files before reading into memory.
    let metadata =
        std::fs::metadata(path).context(format!("Failed to read manifest: {manifest_path}"))?;
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

    // #379: load the cost weights once when cost enforcement is requested.
    let cost_weights: HashMap<String, usize> = match (max_cost, schema_path) {
        (Some(_), Some(path)) => {
            let json = std::fs::read_to_string(path)
                .context(format!("Failed to read compiled schema: {path}"))?;
            fraiseql_core::schema::CompiledSchema::from_json(&json, false)
                .map_err(|e| anyhow::anyhow!("Failed to parse compiled schema {path}: {e}"))?
                .operation_cost_weights
        },
        _ => HashMap::new(),
    };

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
        let computed = hex::encode(Sha256::digest(body.as_bytes()));
        if computed != hash_hex {
            results.push(EntryResult {
                key:   key.clone(),
                valid: false,
                error: Some(format!("Hash mismatch: computed {computed}")),
            });
            continue;
        }

        // #379: score hash-valid documents against the cost ceiling.
        if let Some(cap) = max_cost {
            match fraiseql_core::graphql::parse_graphql_document(body) {
                Ok(doc) => {
                    let cost =
                        fraiseql_core::graphql::estimate_query_cost(&doc, &cost_weights, None)
                            as u64;
                    formatter.progress(&format!("  {key} - cost {cost}"));
                    if cost > cap {
                        results.push(EntryResult {
                            key:   key.clone(),
                            valid: false,
                            error: Some(format!("Estimated cost {cost} exceeds --max-cost {cap}")),
                        });
                        continue;
                    }
                },
                Err(e) => {
                    results.push(EntryResult {
                        key:   key.clone(),
                        valid: false,
                        error: Some(format!("Not parseable as GraphQL: {e}")),
                    });
                    continue;
                },
            }
        }

        results.push(EntryResult {
            key:   key.clone(),
            valid: true,
            error: None,
        });
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
