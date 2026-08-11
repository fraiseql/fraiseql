//! Transform result caching (#973).
//!
//! A render is a pure function of the source bytes and the resolved transform,
//! so the cache is **content-addressed**: the entry's key contains a digest of
//! both. That is the whole invalidation story — a re-uploaded source hashes
//! differently and therefore reads a different key, so a stale entry can never
//! be served and there is nothing to invalidate.
//!
//! The shape this replaces had an `invalidate()` that wrote a marker no reader
//! ever consulted: an invalidation that looked like one and was not. Content
//! addressing removes the need for the method rather than fixing it.
//!
//! Entries live under the reserved `.fraiseql-transforms/` prefix. A caller's
//! object always lands at `{logical-bucket}/{key}` and every key is validated
//! to be a relative path with no `.`/`..` segment, so the only way a caller
//! could name a cache entry is if a bucket were *configured* with the reserved
//! name — which the server refuses at boot
//! ([`RESERVED_BUCKET_NAMES`](crate::config::RESERVED_BUCKET_NAMES)). A cache a
//! caller can write into is a cache a caller can poison.

use std::sync::Arc;

use fraiseql_error::Result;
use sha2::{Digest, Sha256};

use super::{TransformOutput, TransformParams};
use crate::backend::StorageBackend;

/// The reserved namespace cache entries live in. Client keys carrying it are
/// refused by `validate_key`, so nothing a caller uploads can land here.
pub const CACHE_PREFIX: &str = ".fraiseql-transforms";

/// Cache of rendered images, stored in the configured storage backend.
pub struct TransformCache {
    backend: Arc<StorageBackend>,
}

impl TransformCache {
    /// Creates a transform cache over the configured storage backend.
    #[must_use]
    pub const fn new(backend: Arc<StorageBackend>) -> Self {
        Self { backend }
    }

    /// The content-addressed key for one rendering.
    ///
    /// The digest covers the source bytes and the canonical description of the
    /// resolved transform, so two renders share a key exactly when they would
    /// produce the same bytes. `bucket` and `key` are carried in the path only
    /// to keep the namespace browsable — they are inside the digest too.
    #[must_use]
    pub fn build_cache_key(
        bucket: &str,
        key: &str,
        source: &[u8],
        params: &TransformParams,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bucket.as_bytes());
        hasher.update([0]);
        hasher.update(key.as_bytes());
        hasher.update([0]);
        hasher.update(source);
        hasher.update([0]);
        hasher.update(params.describe().as_bytes());
        let digest = hex::encode(hasher.finalize());
        // Two levels of fan-out keep any one directory small on the local
        // backend, which is a real filesystem.
        format!("{CACHE_PREFIX}/{}/{}/{digest}", &digest[0..2], &digest[2..4])
    }

    /// Read a cached rendering, if one is stored.
    ///
    /// A miss and an unreadable entry are the same thing to the caller: render
    /// it again. A cache is never the reason a request fails.
    pub async fn get(&self, cache_key: &str) -> Option<TransformOutput> {
        let raw = self.backend.download(cache_key).await.ok()?;
        match serde_json::from_slice(&raw) {
            Ok(output) => Some(output),
            Err(e) => {
                tracing::warn!(cache_key, error = %e, "Discarding an unreadable transform-cache entry");
                None
            },
        }
    }

    /// Store a rendering.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the backend write fails. Callers treat
    /// that as non-fatal — the render already succeeded.
    pub async fn put(&self, cache_key: &str, output: &TransformOutput) -> Result<()> {
        let serialized = serde_json::to_vec(output)?;
        self.backend.upload(cache_key, &serialized, "application/json").await?;
        Ok(())
    }
}
