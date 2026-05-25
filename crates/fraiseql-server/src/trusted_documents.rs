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
/// 10 `MiB` — already an unusually large deployment.  Capping at 10 `MiB` prevents
/// accidental or malicious loading of a gigabyte-sized file at server startup.
pub(crate) const MAX_MANIFEST_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB

use dashmap::DashMap;
use serde::Deserialize;

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
    #[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
    version: u32,
    documents: HashMap<String, String>,
}

/// Trusted document lookup store.
///
/// Backed by a [`DashMap`]: lookups on the request hot path take only a
/// per-shard read lock, never an async lock, and hot-reload (the only writer)
/// replaces entries in place without blocking concurrent readers on other
/// shards.
pub struct TrustedDocumentStore {
    /// hash → query body (keys stored WITHOUT "sha256:" prefix).
    documents: Arc<DashMap<String, String>>,
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
        Ok(Self {
            documents: Arc::new(normalize_keys(manifest.documents)),
            mode,
        })
    }

    /// Create an in-memory store from a pre-built document map (for testing).
    #[must_use]
    pub fn from_documents(documents: HashMap<String, String>, mode: TrustedDocumentMode) -> Self {
        Self {
            documents: Arc::new(normalize_keys(documents)),
            mode,
        }
    }

    /// A disabled store that passes all queries through (permissive, empty).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            documents: Arc::new(DashMap::new()),
            mode:      TrustedDocumentMode::Permissive,
        }
    }

    /// Returns the enforcement mode.
    #[must_use]
    pub const fn mode(&self) -> TrustedDocumentMode {
        self.mode
    }

    /// Returns the number of documents in the manifest.
    #[must_use]
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Replace the document map (used by hot-reload).
    ///
    /// The swap is per-shard atomic — readers may observe the old or the new
    /// contents but never a torn entry.  Brief inconsistency across shards
    /// during a reload is acceptable: a request that resolves a document
    /// that has just been removed will simply 404 and the client will retry.
    pub fn replace_documents(&self, documents: HashMap<String, String>) {
        let new_docs = normalize_keys(documents);
        self.documents.clear();
        for entry in new_docs {
            self.documents.insert(entry.0, entry.1);
        }
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
    pub fn resolve(
        &self,
        document_id: Option<&str>,
        raw_query: Option<&str>,
    ) -> Result<String, TrustedDocumentError> {
        if let Some(doc_id) = document_id {
            let hash = doc_id.strip_prefix("sha256:").unwrap_or(doc_id);
            return self.documents.get(hash).map(|r| r.value().clone()).ok_or_else(|| {
                TrustedDocumentError::DocumentNotFound {
                    id: doc_id.to_string(),
                }
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
fn normalize_keys(documents: HashMap<String, String>) -> DashMap<String, String> {
    let out = DashMap::with_capacity(documents.len());
    for (k, v) in documents {
        let key = k.strip_prefix("sha256:").unwrap_or(&k).to_string();
        out.insert(key, v);
    }
    out
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
