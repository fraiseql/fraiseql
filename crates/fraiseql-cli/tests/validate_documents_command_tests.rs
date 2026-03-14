#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests for `fraiseql validate-documents`.
//!
//! The command validates a trusted-documents manifest JSON:
//! - Each key must be `sha256:<64-hex-chars>`
//! - The hash must match `SHA-256(query_body)`
//!
//! **Execution engine:** none (CLI binary only)
//! **Infrastructure:** none (filesystem only)
//! **Parallelism:** safe

use sha2::{Digest, Sha256};
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Write a manifest JSON and return its path (inside a tempdir).
fn write_manifest(dir: &tempfile::TempDir, content: &serde_json::Value) -> String {
    let path = dir.path().join("manifest.json");
    std::fs::write(&path, serde_json::to_string(content).unwrap()).unwrap();
    path.to_str().unwrap().to_string()
}

/// Compute the `sha256:` prefixed key for a query body.
fn sha256_key(query: &str) -> String {
    let hash = format!("{:x}", Sha256::digest(query.as_bytes()));
    format!("sha256:{hash}")
}

// ── validate-documents basic ─────────────────────────────────────

/// A valid manifest (correct hash → body mapping) exits 0.
#[test]
fn validate_documents_valid_manifest_exits_zero() {
    let out = cli()
        .args(["validate-documents", &fixture("valid_manifest.json")])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "valid manifest must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Valid manifest output (stderr) contains the document count.
#[test]
fn validate_documents_prints_document_count() {
    let out = cli()
        .args(["validate-documents", &fixture("valid_manifest.json")])
        .output()
        .unwrap();
    // The CLI writes progress output to stderr (via OutputFormatter).
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Either the count line or an "All documents valid" line must appear
    assert!(
        combined.contains('1') || combined.contains("valid"),
        "output must mention count or validity; got: {combined}"
    );
}

/// Missing manifest file exits 1 (error).
#[test]
fn validate_documents_missing_file_exits_nonzero() {
    let out = cli()
        .args(["validate-documents", "does_not_exist.json"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "missing manifest must exit non-zero"
    );
}

// ── Hash validation ───────────────────────────────────────────────

/// A manifest with a mismatched hash exits 2 (validation failure).
#[test]
fn validate_documents_mismatched_hash_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = serde_json::json!({
        "version": 1,
        "documents": {
            // All-zero hash does not match the query body
            "sha256:0000000000000000000000000000000000000000000000000000000000000000":
                "query { users { id } }"
        }
    });
    let path = write_manifest(&dir, &manifest);
    let out = cli().args(["validate-documents", &path]).output().unwrap();
    assert!(!out.status.success(), "mismatched hash must exit non-zero");
}

/// A manifest with an invalid hash format (too short) exits non-zero.
#[test]
fn validate_documents_invalid_hash_format_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = serde_json::json!({
        "version": 1,
        "documents": {
            "sha256:short": "query { users { id } }"
        }
    });
    let path = write_manifest(&dir, &manifest);
    let out = cli().args(["validate-documents", &path]).output().unwrap();
    assert!(!out.status.success(), "invalid hash format must exit non-zero");
}

/// A manifest where all hashes are correct — multiple documents.
#[test]
fn validate_documents_multiple_valid_entries_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let q1 = "query GetUsers { users { id email } }";
    let q2 = "mutation CreateUser($email: String!) { createUser(email: $email) { id } }";
    let manifest = serde_json::json!({
        "version": 1,
        "documents": {
            sha256_key(q1): q1,
            sha256_key(q2): q2
        }
    });
    let path = write_manifest(&dir, &manifest);
    let out = cli().args(["validate-documents", &path]).output().unwrap();
    assert!(
        out.status.success(),
        "manifest with multiple correct hashes must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A manifest where one of two hashes is wrong exits non-zero.
#[test]
fn validate_documents_partial_mismatch_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let good_query = "query { users { id } }";
    let manifest = serde_json::json!({
        "version": 1,
        "documents": {
            // correct
            sha256_key(good_query): good_query,
            // incorrect: key doesn't match its body
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa":
                "query { differentQuery { name } }"
        }
    });
    let path = write_manifest(&dir, &manifest);
    let out = cli().args(["validate-documents", &path]).output().unwrap();
    assert!(!out.status.success(), "partial hash mismatch must exit non-zero");
}

/// A manifest with no `sha256:` prefix on the key is also validated (prefix optional).
#[test]
fn validate_documents_hash_without_prefix_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let query = "query GetOrders { orders { id status } }";
    let hash = format!("{:x}", Sha256::digest(query.as_bytes()));
    // Key without `sha256:` prefix
    let manifest = serde_json::json!({
        "version": 1,
        "documents": {
            hash: query
        }
    });
    let path = write_manifest(&dir, &manifest);
    let out = cli().args(["validate-documents", &path]).output().unwrap();
    assert!(
        out.status.success(),
        "manifest with unprefixed hash must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
