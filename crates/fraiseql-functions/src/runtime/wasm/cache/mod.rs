//! LRU cache for pre-compiled WASM components.
//!
//! Compiling a WASM module from bytecode is the most expensive part of a cold
//! start (typically 10–200 ms depending on module size).  This cache stores
//! the result of compilation keyed by the **content hash** of the bytecode,
//! so re-deploying the same function artifact reuses the compiled component
//! without recompilation.
//!
//! # Key design choices
//!
//! - **Cache key = `source_hash`** (SHA-256 of bytecode).  Content-addressed
//!   keys mean cache entries are automatically invalidated when the bytecode
//!   changes; there is no need for explicit version tracking.
//! - **`Arc<Component>`** — `wasmtime::component::Component` is `Send + Sync`
//!   and cheap to clone via `Arc`, so we hand out shared references without
//!   copying compiled code.
//! - **`Mutex<LruCache>`** — the hot path is per-invocation lookup; a single
//!   `std::sync::Mutex` is sufficient because compilation time dominates any
//!   contention on the lock.

use std::sync::{Arc, Mutex};

use lru::LruCache;
use wasmtime::component::Component;

/// Default maximum number of pre-compiled WASM components to keep in the cache.
pub const DEFAULT_MODULE_CACHE_SIZE: usize = 64;

/// Thread-safe LRU cache of pre-compiled WASM components.
///
/// Components are keyed by the SHA-256 content hash of their bytecode, so the
/// cache is automatically invalidated when a function's code changes.
///
/// Clone is cheap — the cache is `Arc`-wrapped internally.
#[derive(Clone)]
pub struct WasmModuleCache {
    inner: Arc<Mutex<LruCache<String, Arc<Component>>>>,
    max_entries: usize,
}

impl std::fmt::Debug for WasmModuleCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.inner.lock().map(|g| g.len()).unwrap_or(0);
        f.debug_struct("WasmModuleCache")
            .field("cached_modules", &len)
            .field("max_entries", &self.max_entries)
            .finish()
    }
}

impl WasmModuleCache {
    /// Create a new cache that holds at most `max_entries` compiled components.
    ///
    /// # Panics
    ///
    /// Panics if `max_entries` is zero (required by `LruCache::new`).
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        let cap = std::num::NonZeroUsize::new(max_entries)
            .expect("WasmModuleCache max_entries must be > 0");
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(cap))),
            max_entries,
        }
    }

    /// Create a cache with the default size of [`DEFAULT_MODULE_CACHE_SIZE`] entries.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MODULE_CACHE_SIZE)
    }

    /// Look up a compiled component by its bytecode content hash.
    ///
    /// Returns `Some(Arc<Component>)` on a cache hit, `None` on a miss.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned (should never happen in normal
    /// operation).
    pub fn get(&self, source_hash: &str) -> Option<Arc<Component>> {
        self.inner
            .lock()
            .expect("WasmModuleCache mutex poisoned")
            .get(source_hash)
            .cloned()
    }

    /// Insert a compiled component into the cache.
    ///
    /// If the cache is full, the least-recently-used entry is evicted.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn insert(&self, source_hash: String, component: Arc<Component>) {
        self.inner
            .lock()
            .expect("WasmModuleCache mutex poisoned")
            .put(source_hash, component);
    }

    /// Return the current number of cached components.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("WasmModuleCache mutex poisoned")
            .len()
    }

    /// Return `true` if the cache contains no entries.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the configured maximum number of entries.
    #[must_use]
    pub const fn max_entries(&self) -> usize {
        self.max_entries
    }
}

#[cfg(test)]
mod tests;
