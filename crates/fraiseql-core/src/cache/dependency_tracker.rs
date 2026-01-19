//! Dependency tracking for cache invalidation.
//!
//! Tracks which cache entries depend on which database views/tables to enable
//! efficient view-based invalidation when mutations occur.
//!
//! # Current Scope
//!
//! - View-based tracking (not entity-level)
//! - Bidirectional mapping (cache ↔ views)
//! - Simple dependency management
//!
//! # Future Enhancements
//!
//! - Entity-level tracking (`User:123` not just `v_user`)
//! - Cascade integration (parse mutation metadata)
//! - Coherency validation (ensure no stale reads)

use std::collections::{HashMap, HashSet};

/// Tracks which cache entries depend on which views/tables.
///
/// Maintains bidirectional mappings between cache keys and the database
/// views/tables they access. This enables efficient lookup during invalidation:
/// "Which cache entries read from `v_user`?"
///
/// # Example
///
/// ```
/// use fraiseql_core::cache::DependencyTracker;
///
/// let mut tracker = DependencyTracker::new();
///
/// // Record that cache entry accesses v_user
/// tracker.record_access(
///     "cache_key_abc123".to_string(),
///     vec!["v_user".to_string()]
/// );
///
/// // Find all caches that read from v_user
/// let affected = tracker.get_dependent_caches("v_user");
/// assert!(affected.contains(&"cache_key_abc123".to_string()));
/// ```
#[derive(Debug)]
pub struct DependencyTracker {
    /// Cache key → list of views accessed.
    ///
    /// Forward mapping for removing cache entries.
    cache_to_views: HashMap<String, Vec<String>>,

    /// View name → set of cache keys that read it.
    ///
    /// Reverse mapping for finding affected caches during invalidation.
    view_to_caches: HashMap<String, HashSet<String>>,
}

impl DependencyTracker {
    /// Create new dependency tracker.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let tracker = DependencyTracker::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache_to_views: HashMap::new(),
            view_to_caches: HashMap::new(),
        }
    }

    /// Record that a cache entry accesses certain views.
    ///
    /// Updates both forward and reverse mappings.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key (from `generate_cache_key()`)
    /// * `views` - List of views accessed by the query
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    ///
    /// tracker.record_access(
    ///     "key1".to_string(),
    ///     vec!["v_user".to_string(), "v_post".to_string()]
    /// );
    /// ```
    pub fn record_access(&mut self, cache_key: String, views: Vec<String>) {
        // If updating existing entry, remove old reverse mappings first
        if let Some(old_views) = self.cache_to_views.get(&cache_key) {
            for old_view in old_views {
                if let Some(caches) = self.view_to_caches.get_mut(old_view) {
                    caches.remove(&cache_key);
                    // Clean up empty sets
                    if caches.is_empty() {
                        self.view_to_caches.remove(old_view);
                    }
                }
            }
        }

        // Store cache → views mapping (forward)
        self.cache_to_views.insert(cache_key.clone(), views.clone());

        // Update view → caches reverse mapping
        for view in views {
            self.view_to_caches
                .entry(view)
                .or_insert_with(HashSet::new)
                .insert(cache_key.clone());
        }
    }

    /// Get all cache keys that access a view.
    ///
    /// Used during invalidation to find affected cache entries.
    ///
    /// # Arguments
    ///
    /// * `view` - View/table name
    ///
    /// # Returns
    ///
    /// List of cache keys that read from this view
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    /// tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
    ///
    /// let affected = tracker.get_dependent_caches("v_user");
    /// assert_eq!(affected.len(), 1);
    /// assert!(affected.contains(&"key1".to_string()));
    /// ```
    #[must_use]
    pub fn get_dependent_caches(&self, view: &str) -> Vec<String> {
        self.view_to_caches
            .get(view)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Remove a cache entry from tracking.
    ///
    /// Called when cache entry is evicted (LRU) or invalidated.
    /// Cleans up both forward and reverse mappings.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key to remove
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    /// tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
    ///
    /// tracker.remove_cache("key1");
    ///
    /// let affected = tracker.get_dependent_caches("v_user");
    /// assert_eq!(affected.len(), 0);
    /// ```
    pub fn remove_cache(&mut self, cache_key: &str) {
        if let Some(views) = self.cache_to_views.remove(cache_key) {
            // Remove from view → caches mappings
            for view in views {
                if let Some(caches) = self.view_to_caches.get_mut(&view) {
                    caches.remove(cache_key);
                    // Clean up empty sets
                    if caches.is_empty() {
                        self.view_to_caches.remove(&view);
                    }
                }
            }
        }
    }

    /// Clear all tracking data.
    ///
    /// Used for testing and cache flush.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    /// tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
    ///
    /// tracker.clear();
    ///
    /// assert_eq!(tracker.cache_count(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.cache_to_views.clear();
        self.view_to_caches.clear();
    }

    /// Get total number of tracked cache entries.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    /// tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
    /// tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);
    ///
    /// assert_eq!(tracker.cache_count(), 2);
    /// ```
    #[must_use]
    pub fn cache_count(&self) -> usize {
        self.cache_to_views.len()
    }

    /// Get total number of tracked views.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    /// tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
    /// tracker.record_access("key2".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);
    ///
    /// assert_eq!(tracker.view_count(), 2);  // v_user and v_post
    /// ```
    #[must_use]
    pub fn view_count(&self) -> usize {
        self.view_to_caches.len()
    }

    /// Get all tracked views.
    ///
    /// Used for debugging and monitoring.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::cache::DependencyTracker;
    ///
    /// let mut tracker = DependencyTracker::new();
    /// tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
    ///
    /// let views = tracker.get_all_views();
    /// assert!(views.contains(&"v_user".to_string()));
    /// ```
    #[must_use]
    pub fn get_all_views(&self) -> Vec<String> {
        self.view_to_caches.keys().cloned().collect()
    }
}

impl Default for DependencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_get_dependency() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);

        let affected = tracker.get_dependent_caches("v_user");
        assert_eq!(affected.len(), 1);
        assert!(affected.contains(&"key1".to_string()));
    }

    #[test]
    fn test_multiple_caches_same_view() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_user".to_string()]);

        let affected = tracker.get_dependent_caches("v_user");
        assert_eq!(affected.len(), 2);
        assert!(affected.contains(&"key1".to_string()));
        assert!(affected.contains(&"key2".to_string()));
    }

    #[test]
    fn test_cache_accesses_multiple_views() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);

        // Should appear in both view mappings
        let user_caches = tracker.get_dependent_caches("v_user");
        let post_caches = tracker.get_dependent_caches("v_post");

        assert!(user_caches.contains(&"key1".to_string()));
        assert!(post_caches.contains(&"key1".to_string()));
    }

    #[test]
    fn test_remove_cache() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.remove_cache("key1");

        let affected = tracker.get_dependent_caches("v_user");
        assert_eq!(affected.len(), 0);
    }

    #[test]
    fn test_remove_cache_with_multiple_views() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);

        tracker.remove_cache("key1");

        // Should be removed from both mappings
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 0);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_cache() {
        let mut tracker = DependencyTracker::new();

        // Should not panic
        tracker.remove_cache("nonexistent");
    }

    #[test]
    fn test_get_nonexistent_view() {
        let tracker = DependencyTracker::new();

        let affected = tracker.get_dependent_caches("nonexistent");
        assert_eq!(affected.len(), 0);
    }

    #[test]
    fn test_clear() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);

        tracker.clear();

        assert_eq!(tracker.cache_count(), 0);
        assert_eq!(tracker.view_count(), 0);
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 0);
    }

    #[test]
    fn test_cache_count() {
        let mut tracker = DependencyTracker::new();

        assert_eq!(tracker.cache_count(), 0);

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        assert_eq!(tracker.cache_count(), 1);

        tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);
        assert_eq!(tracker.cache_count(), 2);

        tracker.remove_cache("key1");
        assert_eq!(tracker.cache_count(), 1);
    }

    #[test]
    fn test_view_count() {
        let mut tracker = DependencyTracker::new();

        assert_eq!(tracker.view_count(), 0);

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        assert_eq!(tracker.view_count(), 1);

        tracker.record_access("key2".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);
        assert_eq!(tracker.view_count(), 2); // v_user and v_post
    }

    #[test]
    fn test_get_all_views() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_post".to_string()]);

        let views = tracker.get_all_views();
        assert_eq!(views.len(), 2);
        assert!(views.contains(&"v_user".to_string()));
        assert!(views.contains(&"v_post".to_string()));
    }

    #[test]
    fn test_update_access_overwrites() {
        let mut tracker = DependencyTracker::new();

        // Initial access
        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);

        // Update to different views
        tracker.record_access("key1".to_string(), vec!["v_post".to_string()]);

        // Should only be in v_post now
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 0);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 1);
    }

    #[test]
    fn test_bidirectional_consistency() {
        let mut tracker = DependencyTracker::new();

        tracker.record_access("key1".to_string(), vec!["v_user".to_string()]);
        tracker.record_access("key2".to_string(), vec!["v_user".to_string(), "v_post".to_string()]);

        // Forward: 2 cache entries
        assert_eq!(tracker.cache_count(), 2);

        // Reverse: v_user has 2 dependencies, v_post has 1
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 2);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 1);

        // Remove one
        tracker.remove_cache("key1");

        // Consistency check
        assert_eq!(tracker.cache_count(), 1);
        assert_eq!(tracker.get_dependent_caches("v_user").len(), 1);
        assert_eq!(tracker.get_dependent_caches("v_post").len(), 1);
    }
}
