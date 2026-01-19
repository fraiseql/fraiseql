//! Query compilation result caching with LRU eviction.
//!
//! This module provides caching for schema compilation results, avoiding redundant
//! compilation of identical schemas. Uses fingerprinting (SHA-256) for cache key generation.

use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    schema::CompiledSchema,
};

/// Cache entry for compiled schema with metadata.
#[derive(Debug, Clone)]
pub struct CachedCompilation {
    /// The compiled schema result.
    pub schema: Arc<CompiledSchema>,

    /// Number of cache hits for this entry.
    pub hit_count: u64,
}

/// Configuration for compilation cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationCacheConfig {
    /// Enable compilation caching.
    pub enabled: bool,

    /// Maximum number of compiled schemas to cache.
    pub max_entries: usize,
}

impl Default for CompilationCacheConfig {
    fn default() -> Self {
        Self {
            enabled:      true,
            max_entries:  100,
        }
    }
}

impl CompilationCacheConfig {
    /// Create disabled cache configuration.
    ///
    /// Useful for deterministic testing where compilation should always occur.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled:      false,
            max_entries:  0,
        }
    }
}

/// Thread-safe LRU cache for compiled schemas.
///
/// # Design
///
/// - **Fingerprinting**: Uses SHA-256 hash of schema JSON for cache keys
/// - **LRU eviction**: Automatically evicts least-recently-used entries
/// - **Thread-safe**: All operations use interior mutability
///
/// # Memory Safety
///
/// - Hard LRU limit ensures bounded memory usage
/// - Default config: 100 entries (reasonable for most deployments)
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::compiler::compilation_cache::{CompilationCache, CompilationCacheConfig};
/// use fraiseql_core::compiler::Compiler;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let cache = CompilationCache::new(CompilationCacheConfig::default());
/// let compiler = Compiler::new();
///
/// let schema_json = r#"{"types": [], "queries": []}"#;
///
/// // First compilation - cache miss
/// let compiled = cache.compile(&compiler, schema_json)?;
///
/// // Second compilation - cache hit
/// let compiled_cached = cache.compile(&compiler, schema_json)?;
///
/// # Ok(())
/// # }
/// ```
pub struct CompilationCache {
    /// LRU cache: fingerprint -> compiled schema.
    cache: Arc<Mutex<lru::LruCache<String, CachedCompilation>>>,

    /// Configuration.
    config: CompilationCacheConfig,

    /// Metrics.
    metrics: Arc<Mutex<CompilationCacheMetrics>>,
}

/// Metrics for compilation cache monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationCacheMetrics {
    /// Number of cache hits.
    pub hits: u64,

    /// Number of cache misses.
    pub misses: u64,

    /// Total compilations performed.
    pub total_compilations: u64,

    /// Current cache size (entries).
    pub size: usize,
}

impl CompilationCache {
    /// Create new compilation cache with configuration.
    ///
    /// # Panics
    ///
    /// Panics if cache is enabled but `max_entries` is 0.
    #[must_use]
    pub fn new(config: CompilationCacheConfig) -> Self {
        if config.enabled {
            let max = NonZeroUsize::new(config.max_entries)
                .expect("max_entries must be > 0 when cache is enabled");
            Self {
                cache: Arc::new(Mutex::new(lru::LruCache::new(max))),
                config,
                metrics: Arc::new(Mutex::new(CompilationCacheMetrics {
                    hits:                  0,
                    misses:                0,
                    total_compilations:    0,
                    size:                  0,
                })),
            }
        } else {
            // Create dummy cache (won't be used)
            let max = NonZeroUsize::new(1).expect("impossible");
            Self {
                cache: Arc::new(Mutex::new(lru::LruCache::new(max))),
                config,
                metrics: Arc::new(Mutex::new(CompilationCacheMetrics {
                    hits:                  0,
                    misses:                0,
                    total_compilations:    0,
                    size:                  0,
                })),
            }
        }
    }

    /// Compute SHA-256 fingerprint of schema JSON.
    ///
    /// This fingerprint uniquely identifies the schema and is used as cache key.
    fn fingerprint(schema_json: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(schema_json.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Compile schema with caching.
    ///
    /// If cache is enabled and schema fingerprint matches a cached entry,
    /// returns the cached compiled schema. Otherwise, compiles the schema
    /// and stores result in cache.
    ///
    /// # Arguments
    ///
    /// * `compiler` - Schema compiler
    /// * `schema_json` - JSON schema from Python/TypeScript decorators
    ///
    /// # Returns
    ///
    /// Compiled schema (cached if possible)
    pub fn compile(
        &self,
        compiler: &crate::compiler::Compiler,
        schema_json: &str,
    ) -> Result<Arc<CompiledSchema>> {
        if !self.config.enabled {
            // Cache disabled - always compile
            let schema = Arc::new(compiler.compile(schema_json)?);

            let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
            metrics.total_compilations += 1;
            metrics.misses += 1;

            return Ok(schema);
        }

        let fingerprint = Self::fingerprint(schema_json);

        // Check cache
        {
            let mut cache = self.cache.lock().expect("cache lock poisoned");
            if let Some(cached) = cache.get_mut(&fingerprint) {
                // Cache hit
                let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
                metrics.hits += 1;
                cached.hit_count += 1;
                return Ok(Arc::clone(&cached.schema));
            }
        }

        // Cache miss - compile schema
        let schema = Arc::new(compiler.compile(schema_json)?);

        // Store in cache
        {
            let mut cache = self.cache.lock().expect("cache lock poisoned");
            cache.put(
                fingerprint,
                CachedCompilation {
                    schema: Arc::clone(&schema),
                    hit_count: 0,
                },
            );

            let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
            metrics.total_compilations += 1;
            metrics.misses += 1;
            metrics.size = cache.len();
        }

        Ok(schema)
    }

    /// Get current cache metrics.
    pub fn metrics(&self) -> Result<CompilationCacheMetrics> {
        let metrics = self.metrics.lock().expect("metrics lock poisoned");
        Ok(metrics.clone())
    }

    /// Clear all cached compilations.
    pub fn clear(&self) -> Result<()> {
        self.cache.lock().expect("cache lock poisoned").clear();
        let mut metrics = self.metrics.lock().expect("metrics lock poisoned");
        metrics.size = 0;
        Ok(())
    }

    /// Get cache hit rate as percentage (0-100).
    pub fn hit_rate(&self) -> Result<f64> {
        let metrics = self.metrics.lock().expect("metrics lock poisoned");
        if metrics.total_compilations == 0 {
            return Ok(0.0);
        }
        Ok((metrics.hits as f64 / metrics.total_compilations as f64) * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_deterministic() {
        let schema = r#"{"types": [], "queries": []}"#;
        let fp1 = CompilationCache::fingerprint(schema);
        let fp2 = CompilationCache::fingerprint(schema);
        assert_eq!(fp1, fp2, "Fingerprints should be deterministic");
    }

    #[test]
    fn test_fingerprint_unique() {
        let schema1 = r#"{"types": [], "queries": []}"#;
        let schema2 = r#"{"types": [{"name": "User"}], "queries": []}"#;
        let fp1 = CompilationCache::fingerprint(schema1);
        let fp2 = CompilationCache::fingerprint(schema2);
        assert_ne!(fp1, fp2, "Different schemas should have different fingerprints");
    }

    #[test]
    fn test_cache_new_enabled() {
        let config = CompilationCacheConfig {
            enabled: true,
            max_entries: 50,
        };
        let cache = CompilationCache::new(config);
        assert!(cache.config.enabled);
    }

    #[test]
    fn test_cache_new_disabled() {
        let config = CompilationCacheConfig::disabled();
        let cache = CompilationCache::new(config);
        assert!(!cache.config.enabled);
    }

    #[test]
    fn test_cache_default_config() {
        let config = CompilationCacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 100);
    }

    #[test]
    fn test_metrics_initial_state() {
        let cache = CompilationCache::new(CompilationCacheConfig::default());
        let metrics = cache.metrics().expect("metrics should work");
        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.total_compilations, 0);
        assert_eq!(metrics.size, 0);
    }

    #[test]
    fn test_hit_rate_no_compilations() {
        let cache = CompilationCache::new(CompilationCacheConfig::default());
        let rate = cache.hit_rate().expect("hit_rate should work");
        assert_eq!(rate, 0.0);
    }

    #[test]
    fn test_clear_cache() {
        let cache = CompilationCache::new(CompilationCacheConfig::default());

        // Disable cache temporarily to add entry without using compile()
        // For now, just verify clear works without panicking
        cache.clear().expect("clear should work");

        let metrics = cache.metrics().expect("metrics should work");
        assert_eq!(metrics.size, 0);
    }

    #[test]
    fn test_cache_config_max_entries_zero_when_disabled() {
        // When cache is disabled, max_entries being 0 is OK
        let config = CompilationCacheConfig {
            enabled: false,
            max_entries: 0,
        };
        let cache = CompilationCache::new(config);
        assert!(!cache.config.enabled);
    }

    #[test]
    #[should_panic(expected = "max_entries must be > 0 when cache is enabled")]
    fn test_cache_panics_on_zero_max_entries_when_enabled() {
        let config = CompilationCacheConfig {
            enabled: true,
            max_entries: 0,
        };
        let _ = CompilationCache::new(config);
    }

    #[test]
    fn test_cache_metrics_clone() {
        let metrics = CompilationCacheMetrics {
            hits: 5,
            misses: 3,
            total_compilations: 8,
            size: 2,
        };
        let cloned = metrics.clone();
        assert_eq!(cloned.hits, 5);
        assert_eq!(cloned.misses, 3);
    }

    #[test]
    fn test_cache_config_serialize() {
        let config = CompilationCacheConfig {
            enabled: true,
            max_entries: 50,
        };
        let json = serde_json::to_string(&config).expect("serialize should work");
        let restored: CompilationCacheConfig =
            serde_json::from_str(&json).expect("deserialize should work");
        assert_eq!(restored.enabled, config.enabled);
        assert_eq!(restored.max_entries, config.max_entries);
    }

    #[test]
    fn test_compilation_cache_metrics_serialize() {
        let metrics = CompilationCacheMetrics {
            hits: 10,
            misses: 5,
            total_compilations: 15,
            size: 3,
        };
        let json = serde_json::to_string(&metrics).expect("serialize should work");
        let restored: CompilationCacheMetrics =
            serde_json::from_str(&json).expect("deserialize should work");
        assert_eq!(restored.hits, 10);
        assert_eq!(restored.size, 3);
    }
}
