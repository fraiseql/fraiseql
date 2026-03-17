//! Trusted documents / query allowlist.
//!
//! Trusted documents allow only pre-registered queries to execute. At build time
//! the frontend generates a manifest keyed by SHA-256 hash. At runtime clients
//! send `{ "documentId": "sha256:abc..." }` instead of a raw query string.
//!
//! Two modes:
//! - **Strict**: only `documentId` requests allowed; raw queries rejected.
//! - **Permissive**: `documentId` resolved from manifest; raw queries pass through.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Maximum byte size accepted for a trusted-documents manifest file.
///
/// A manifest with 50 000 pre-registered queries at ~200 bytes each is roughly
/// 10 MiB — already an unusually large deployment.  Capping at 10 MiB prevents
/// accidental or malicious loading of a gigabyte-sized file at server startup.
const MAX_MANIFEST_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB

use serde::Deserialize;
use tokio::sync::RwLock;

/// Enforcement mode for trusted documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustedDocumentMode {
    /// Only `documentId` requests allowed; raw query strings rejected.
    Strict,
    /// `documentId` requests use the manifest; raw queries fall through.
    Permissive,
}

/// Manifest JSON format (compatible with Relay, Apollo Client, Envelop).
#[derive(Debug, Deserialize)]
struct Manifest {
    // Reason: serde deserialization target — `version` is present in the manifest JSON
    // for forward-compatibility but is not consumed by the current lookup logic.
    #[allow(dead_code)]  // Reason: field kept for API completeness; may be used in future features
    version:   u32,
    documents: HashMap<String, String>,
}

/// Trusted document lookup store.
pub struct TrustedDocumentStore {
    /// hash → query body (keys stored WITHOUT "sha256:" prefix).
    documents: Arc<RwLock<HashMap<String, String>>>,
    mode:      TrustedDocumentMode,
}

impl TrustedDocumentStore {
    /// Load from a JSON manifest file at startup.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_manifest_file(
        path: &Path,
        mode: TrustedDocumentMode,
    ) -> Result<Self, TrustedDocumentError> {
        // Reject oversized files before reading into memory.
        let file_size = std::fs::metadata(path)
            .map_err(|e| {
                TrustedDocumentError::ManifestLoad(format!(
                    "Failed to stat manifest {}: {e}",
                    path.display()
                ))
            })?
            .len();
        if file_size > MAX_MANIFEST_BYTES {
            return Err(TrustedDocumentError::ManifestLoad(format!(
                "Manifest {} is too large ({file_size} bytes, max {MAX_MANIFEST_BYTES})",
                path.display()
            )));
        }

        let contents = std::fs::read_to_string(path).map_err(|e| {
            TrustedDocumentError::ManifestLoad(format!(
                "Failed to read manifest {}: {e}",
                path.display()
            ))
        })?;
        let manifest: Manifest = serde_json::from_str(&contents).map_err(|e| {
            TrustedDocumentError::ManifestLoad(format!(
                "Failed to parse manifest {}: {e}",
                path.display()
            ))
        })?;
        let documents = normalize_keys(manifest.documents);
        Ok(Self {
            documents: Arc::new(RwLock::new(documents)),
            mode,
        })
    }

    /// Create an in-memory store from a pre-built document map (for testing).
    pub fn from_documents(documents: HashMap<String, String>, mode: TrustedDocumentMode) -> Self {
        let documents = normalize_keys(documents);
        Self {
            documents: Arc::new(RwLock::new(documents)),
            mode,
        }
    }

    /// A disabled store that passes all queries through (permissive, empty).
    pub fn disabled() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            mode:      TrustedDocumentMode::Permissive,
        }
    }

    /// Returns the enforcement mode.
    pub const fn mode(&self) -> TrustedDocumentMode {
        self.mode
    }

    /// Returns the number of documents in the manifest.
    pub async fn document_count(&self) -> usize {
        self.documents.read().await.len()
    }

    /// Atomically replace the document map (used by hot-reload).
    pub async fn replace_documents(&self, documents: HashMap<String, String>) {
        let documents = normalize_keys(documents);
        *self.documents.write().await = documents;
    }

    /// Resolve a query from `document_id` and/or `raw_query`.
    ///
    /// - `document_id` present + found → return stored query body.
    /// - `document_id` present + NOT found → `DocumentNotFound`.
    /// - No `document_id` in strict mode → `ForbiddenRawQuery`.
    /// - No `document_id` in permissive mode → return `raw_query`.
    ///
    /// # Errors
    ///
    /// Returns `TrustedDocumentError::DocumentNotFound` if a `document_id` is given but not in the
    /// store. Returns `TrustedDocumentError::ForbiddenRawQuery` if no `document_id` is provided
    /// in strict mode, or if `raw_query` is also absent in permissive mode.
    pub async fn resolve(
        &self,
        document_id: Option<&str>,
        raw_query: Option<&str>,
    ) -> Result<String, TrustedDocumentError> {
        if let Some(doc_id) = document_id {
            let hash = doc_id.strip_prefix("sha256:").unwrap_or(doc_id);
            let docs = self.documents.read().await;
            return docs.get(hash).cloned().ok_or_else(|| TrustedDocumentError::DocumentNotFound {
                id: doc_id.to_string(),
            });
        }
        match self.mode {
            TrustedDocumentMode::Strict => Err(TrustedDocumentError::ForbiddenRawQuery),
            TrustedDocumentMode::Permissive => {
                raw_query.map(|s| s.to_string()).ok_or(TrustedDocumentError::ForbiddenRawQuery)
            },
        }
    }
}

/// Normalize manifest keys: strip "sha256:" prefix for uniform lookup.
fn normalize_keys(documents: HashMap<String, String>) -> HashMap<String, String> {
    documents
        .into_iter()
        .map(|(k, v)| {
            let key = k.strip_prefix("sha256:").unwrap_or(&k).to_string();
            (key, v)
        })
        .collect()
}

/// Errors from trusted document resolution.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TrustedDocumentError {
    /// Raw queries are not permitted in strict mode.
    #[error("Raw queries are not permitted. Send a documentId instead.")]
    ForbiddenRawQuery,

    /// The requested document ID was not found in the manifest.
    #[error("Unknown document: {id}")]
    DocumentNotFound {
        /// The document ID that was requested but not found.
        id: String,
    },

    /// Failed to load the manifest file.
    #[error("Manifest load error: {0}")]
    ManifestLoad(String),
}

// ── Metrics ──────────────────────────────────────────────────────────────

static TRUSTED_DOC_HITS: AtomicU64 = AtomicU64::new(0);
static TRUSTED_DOC_MISSES: AtomicU64 = AtomicU64::new(0);
static TRUSTED_DOC_REJECTED: AtomicU64 = AtomicU64::new(0);

/// Record a trusted document cache hit.
pub fn record_hit() {
    TRUSTED_DOC_HITS.fetch_add(1, Ordering::Relaxed);
}

/// Record a trusted document miss (unknown document ID).
pub fn record_miss() {
    TRUSTED_DOC_MISSES.fetch_add(1, Ordering::Relaxed);
}

/// Record a rejected raw query (strict mode).
pub fn record_rejected() {
    TRUSTED_DOC_REJECTED.fetch_add(1, Ordering::Relaxed);
}

/// Total trusted document hits.
pub fn hits_total() -> u64 {
    TRUSTED_DOC_HITS.load(Ordering::Relaxed)
}

/// Total trusted document misses.
pub fn misses_total() -> u64 {
    TRUSTED_DOC_MISSES.load(Ordering::Relaxed)
}

/// Total rejected raw queries.
pub fn rejected_total() -> u64 {
    TRUSTED_DOC_REJECTED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    fn test_documents() -> HashMap<String, String> {
        let mut docs = HashMap::new();
        docs.insert("sha256:abc123".to_string(), "{ users { id } }".to_string());
        docs.insert("sha256:def456".to_string(), "mutation { createUser { id } }".to_string());
        docs
    }

    #[tokio::test]
    async fn strict_mode_rejects_raw_query() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(None, Some("{ users { id } }")).await;
        assert!(matches!(result, Err(TrustedDocumentError::ForbiddenRawQuery)));
    }

    #[tokio::test]
    async fn strict_mode_accepts_valid_document_id() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(Some("sha256:abc123"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[tokio::test]
    async fn strict_mode_rejects_unknown_document_id() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        let result = store.resolve(Some("sha256:unknown"), None).await;
        assert!(matches!(result, Err(TrustedDocumentError::DocumentNotFound { .. })));
    }

    #[tokio::test]
    async fn permissive_mode_allows_raw_queries() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Permissive);
        let result = store.resolve(None, Some("{ arbitrary { query } }")).await;
        assert_eq!(result.unwrap(), "{ arbitrary { query } }");
    }

    #[tokio::test]
    async fn permissive_mode_uses_manifest_for_document_id() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Permissive);
        let result = store.resolve(Some("sha256:abc123"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[tokio::test]
    async fn document_id_without_prefix_is_resolved() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        // Document ID without "sha256:" prefix should still resolve
        let result = store.resolve(Some("abc123"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    #[tokio::test]
    async fn disabled_store_passes_through() {
        let store = TrustedDocumentStore::disabled();
        let result = store.resolve(None, Some("{ anything }")).await;
        assert_eq!(result.unwrap(), "{ anything }");
    }

    #[tokio::test]
    async fn hot_reload_replaces_documents() {
        let store =
            TrustedDocumentStore::from_documents(test_documents(), TrustedDocumentMode::Strict);
        assert_eq!(store.document_count().await, 2);

        let mut new_docs = HashMap::new();
        new_docs.insert("sha256:new123".to_string(), "{ new query }".to_string());
        store.replace_documents(new_docs).await;

        assert_eq!(store.document_count().await, 1);
        let result = store.resolve(Some("sha256:new123"), None).await;
        assert_eq!(result.unwrap(), "{ new query }");

        // Old document should be gone
        let result = store.resolve(Some("sha256:abc123"), None).await;
        assert!(
            matches!(result, Err(TrustedDocumentError::DocumentNotFound { .. })),
            "expected DocumentNotFound after hot-reload removed old document, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn manifest_file_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trusted-documents.json");
        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                "sha256:aaa": "{ users { id } }",
                "sha256:bbb": "{ posts { title } }"
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let store =
            TrustedDocumentStore::from_manifest_file(&path, TrustedDocumentMode::Strict).unwrap();
        assert_eq!(store.document_count().await, 2);
        let result = store.resolve(Some("sha256:aaa"), None).await;
        assert_eq!(result.unwrap(), "{ users { id } }");
    }

    // ── Manifest size-cap tests ────────────────────────────────────────────────

    #[test]
    fn manifest_file_exceeding_size_limit_is_rejected() {
        use std::io::Write as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge-manifest.json");

        // Write MAX_MANIFEST_BYTES + 1 bytes.
        let mut f = std::fs::File::create(&path).unwrap();
        // Write a JSON object start, then pad with spaces to exceed the limit.
        f.write_all(b"{\"version\":1,\"documents\":{}}").unwrap();
        let padding = vec![b' '; (MAX_MANIFEST_BYTES + 1) as usize];
        f.write_all(&padding).unwrap();
        drop(f);

        let result = TrustedDocumentStore::from_manifest_file(&path, TrustedDocumentMode::Strict);
        assert!(result.is_err(), "oversized manifest must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("too large") || msg.contains("10485760"),
            "error must mention size limit: {msg}"
        );
    }

    #[test]
    fn manifest_file_at_size_limit_is_accepted_if_valid() {
        // A file well under the limit with a valid manifest must still work.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small-manifest.json");
        let manifest = serde_json::json!({"version": 1, "documents": {}});
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();
        TrustedDocumentStore::from_manifest_file(&path, TrustedDocumentMode::Permissive)
            .unwrap_or_else(|e| panic!("small valid manifest must be accepted: {e}"));
    }
}
