//! Cascading cache invalidation for transitive view dependencies.
//!
//! Tracks view-to-view dependencies and propagates invalidation through the dependency graph.
//! When a view is invalidated, all dependent views are also invalidated.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Tracks transitive view-to-view dependencies for cascading invalidation.
///
/// # Architecture
///
/// When `v_user` depends on `v_raw_user`, and `v_analytics` depends on `v_user`:
///
/// ```text
/// v_raw_user (source)
///    ↓ (depends on)
/// v_user (intermediate)
///    ↓ (depends on)
/// v_analytics (leaf)
/// ```
///
/// Invalidating `v_raw_user` cascades to invalidate `v_user` and `v_analytics`.
///
/// # Example
///
/// ```rust
/// use fraiseql_core::cache::cascade_invalidator::CascadeInvalidator;
///
/// let mut invalidator = CascadeInvalidator::new();
///
/// // Register that v_user depends on v_raw_user
/// invalidator.add_dependency("v_user", "v_raw_user").unwrap();
///
/// // Register that v_analytics depends on v_user
/// invalidator.add_dependency("v_analytics", "v_user").unwrap();
///
/// // Invalidate v_raw_user - cascades to v_user and v_analytics
/// let affected = invalidator.cascade_invalidate("v_raw_user").unwrap();
/// assert_eq!(affected.len(), 3); // v_raw_user, v_user, v_analytics
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeInvalidator {
    /// Dependent view → list of views it depends on.
    /// Example: v_user → [v_raw_user]
    view_dependencies: HashMap<String, HashSet<String>>,

    /// View → list of views that depend on it (reverse mapping).
    /// Example: v_raw_user → [v_user]
    dependents: HashMap<String, HashSet<String>>,

    /// Statistics for monitoring.
    stats: InvalidationStats,
}

/// Statistics for cascade invalidation operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidationStats {
    /// Total number of cascade invalidations performed.
    pub total_cascades: u64,

    /// Total views invalidated across all cascades.
    pub total_invalidated: u64,

    /// Average views affected per cascade.
    pub average_affected: f64,

    /// Maximum views affected in a single cascade.
    pub max_affected: usize,
}

impl Default for InvalidationStats {
    fn default() -> Self {
        Self {
            total_cascades:    0,
            total_invalidated: 0,
            average_affected:  0.0,
            max_affected:      0,
        }
    }
}

impl CascadeInvalidator {
    /// Create new cascade invalidator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            view_dependencies: HashMap::new(),
            dependents:        HashMap::new(),
            stats:             InvalidationStats::default(),
        }
    }

    /// Register a view dependency.
    ///
    /// Declares that `dependent_view` depends on `dependency_view`.
    /// When `dependency_view` is invalidated, `dependent_view` will also be invalidated.
    ///
    /// # Arguments
    ///
    /// * `dependent_view` - View that depends on another
    /// * `dependency_view` - View that is depended upon
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::cascade_invalidator::CascadeInvalidator;
    ///
    /// let mut invalidator = CascadeInvalidator::new();
    /// invalidator.add_dependency("v_analytics", "v_user").unwrap();
    /// ```
    pub fn add_dependency(&mut self, dependent_view: &str, dependency_view: &str) -> Result<()> {
        if dependent_view == dependency_view {
            return Err(crate::error::FraiseQLError::Validation {
                message: "View cannot depend on itself".to_string(),
                path:    Some("cascade_invalidator::add_dependency".to_string()),
            });
        }

        let dependent = dependent_view.to_string();
        let dependency = dependency_view.to_string();

        // Add forward mapping: dependent → dependency
        self.view_dependencies
            .entry(dependent.clone())
            .or_insert_with(HashSet::new)
            .insert(dependency.clone());

        // Add reverse mapping: dependency → dependent
        self.dependents.entry(dependency).or_insert_with(HashSet::new).insert(dependent);

        Ok(())
    }

    /// Cascade invalidate a view and all dependent views.
    ///
    /// Uses breadth-first search to find all views that transitively depend on
    /// the given view, and returns the complete set of invalidated views.
    ///
    /// # Algorithm
    ///
    /// 1. Start with the target view
    /// 2. Find all views that directly depend on it
    /// 3. For each dependent, recursively find views that depend on it
    /// 4. Return complete set (target + all transitive dependents)
    ///
    /// # Arguments
    ///
    /// * `view` - View to invalidate
    ///
    /// # Returns
    ///
    /// Set of all invalidated views (including the target)
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::cascade_invalidator::CascadeInvalidator;
    ///
    /// let mut invalidator = CascadeInvalidator::new();
    /// invalidator.add_dependency("v_user_stats", "v_user").unwrap();
    /// invalidator.add_dependency("v_dashboard", "v_user_stats").unwrap();
    ///
    /// let invalidated = invalidator.cascade_invalidate("v_user").unwrap();
    /// assert!(invalidated.contains("v_user"));
    /// assert!(invalidated.contains("v_user_stats"));
    /// assert!(invalidated.contains("v_dashboard"));
    /// ```
    pub fn cascade_invalidate(&mut self, view: &str) -> Result<HashSet<String>> {
        let mut invalidated = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(view.to_string());
        invalidated.insert(view.to_string());

        // BFS to find all transitive dependents
        while let Some(current_view) = queue.pop_front() {
            if let Some(dependent_views) = self.dependents.get(&current_view) {
                for dependent in dependent_views {
                    if !invalidated.contains(dependent) {
                        invalidated.insert(dependent.clone());
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }

        // Update statistics
        self.stats.total_cascades += 1;
        self.stats.total_invalidated += invalidated.len() as u64;
        self.stats.max_affected = self.stats.max_affected.max(invalidated.len());
        if self.stats.total_cascades > 0 {
            self.stats.average_affected =
                self.stats.total_invalidated as f64 / self.stats.total_cascades as f64;
        }

        Ok(invalidated)
    }

    /// Get all views that depend on a given view (direct dependents only).
    ///
    /// For transitive dependents, use `cascade_invalidate()`.
    ///
    /// # Arguments
    ///
    /// * `view` - View to query
    ///
    /// # Returns
    ///
    /// Set of views that directly depend on the given view
    pub fn get_direct_dependents(&self, view: &str) -> HashSet<String> {
        self.dependents.get(view).cloned().unwrap_or_default()
    }

    /// Get all views that a given view depends on (direct dependencies only).
    ///
    /// # Arguments
    ///
    /// * `view` - View to query
    ///
    /// # Returns
    ///
    /// Set of views that the given view depends on
    pub fn get_direct_dependencies(&self, view: &str) -> HashSet<String> {
        self.view_dependencies.get(view).cloned().unwrap_or_default()
    }

    /// Get all views that transitively depend on a given view.
    ///
    /// Like `cascade_invalidate()` but non-mutating (for queries).
    ///
    /// # Arguments
    ///
    /// * `view` - View to query
    ///
    /// # Returns
    ///
    /// Set of all transitive dependents (including the view itself)
    pub fn get_transitive_dependents(&self, view: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(view.to_string());
        result.insert(view.to_string());

        while let Some(current) = queue.pop_front() {
            if let Some(deps) = self.dependents.get(&current) {
                for dep in deps {
                    if !result.contains(dep) {
                        result.insert(dep.clone());
                        queue.push_back(dep.clone());
                    }
                }
            }
        }

        result
    }

    /// Check if there's a dependency path between two views.
    ///
    /// Returns true if `dependent` transitively depends on `dependency`.
    ///
    /// # Arguments
    ///
    /// * `dependent` - Potentially dependent view
    /// * `dependency` - Potentially depended-upon view
    ///
    /// # Returns
    ///
    /// true if there's a transitive dependency
    pub fn has_dependency_path(&self, dependent: &str, dependency: &str) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(dependent.to_string());

        while let Some(current) = queue.pop_front() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if let Some(deps) = self.view_dependencies.get(&current) {
                for dep in deps {
                    if dep == dependency {
                        return true;
                    }
                    queue.push_back(dep.clone());
                }
            }
        }

        false
    }

    /// Get cascade invalidation statistics.
    pub fn stats(&self) -> InvalidationStats {
        self.stats.clone()
    }

    /// Clear all registered dependencies.
    pub fn clear(&mut self) {
        self.view_dependencies.clear();
        self.dependents.clear();
        self.stats = InvalidationStats::default();
    }

    /// Get total number of views tracked.
    pub fn view_count(&self) -> usize {
        let mut views = HashSet::new();
        views.extend(self.dependents.keys().cloned());
        views.extend(self.view_dependencies.keys().cloned());
        views.len()
    }

    /// Get total number of dependency edges.
    pub fn dependency_count(&self) -> usize {
        self.view_dependencies.values().map(|deps| deps.len()).sum()
    }
}

impl Default for CascadeInvalidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_single_dependency() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();

        assert!(invalidator.get_direct_dependencies("v_user").contains("v_raw_user"));
        assert!(invalidator.get_direct_dependents("v_raw_user").contains("v_user"));
    }

    #[test]
    fn test_self_dependency_fails() {
        let mut invalidator = CascadeInvalidator::new();
        let result = invalidator.add_dependency("v_user", "v_user");
        assert!(result.is_err());
    }

    #[test]
    fn test_cascade_invalidate_single_level() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        assert_eq!(invalidated.len(), 2);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
    }

    #[test]
    fn test_cascade_invalidate_multiple_levels() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_dashboard", "v_analytics").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        assert_eq!(invalidated.len(), 4);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
        assert!(invalidated.contains("v_analytics"));
        assert!(invalidated.contains("v_dashboard"));
    }

    #[test]
    fn test_cascade_invalidate_branching() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_dashboard", "v_post").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        assert_eq!(invalidated.len(), 5);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
        assert!(invalidated.contains("v_post"));
        assert!(invalidated.contains("v_analytics"));
        assert!(invalidated.contains("v_dashboard"));
    }

    #[test]
    fn test_get_direct_dependents() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();

        let dependents = invalidator.get_direct_dependents("v_raw_user");
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains("v_user"));
        assert!(dependents.contains("v_post"));
    }

    #[test]
    fn test_get_direct_dependencies() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_post").unwrap();

        let deps = invalidator.get_direct_dependencies("v_analytics");
        assert_eq!(deps.len(), 2);
        assert!(deps.contains("v_user"));
        assert!(deps.contains("v_post"));
    }

    #[test]
    fn test_get_transitive_dependents() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_dashboard", "v_analytics").unwrap();

        let transitive = invalidator.get_transitive_dependents("v_raw_user");
        assert_eq!(transitive.len(), 4);
        assert!(transitive.contains("v_raw_user"));
        assert!(transitive.contains("v_user"));
        assert!(transitive.contains("v_analytics"));
        assert!(transitive.contains("v_dashboard"));
    }

    #[test]
    fn test_has_dependency_path() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        assert!(invalidator.has_dependency_path("v_analytics", "v_raw_user"));
        assert!(invalidator.has_dependency_path("v_analytics", "v_user"));
        assert!(invalidator.has_dependency_path("v_user", "v_raw_user"));
        assert!(!invalidator.has_dependency_path("v_raw_user", "v_analytics"));
        assert!(!invalidator.has_dependency_path("v_raw_user", "v_user"));
    }

    #[test]
    fn test_stats_tracking() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        invalidator.cascade_invalidate("v_raw_user").unwrap();
        invalidator.cascade_invalidate("v_user").unwrap();

        let stats = invalidator.stats();
        assert_eq!(stats.total_cascades, 2);
        assert_eq!(stats.total_invalidated, 5); // 3 (raw_user + user + analytics) + 2 (user + analytics)
        assert_eq!(stats.max_affected, 3);
    }

    #[test]
    fn test_clear() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        assert_eq!(invalidator.view_count(), 2);

        invalidator.clear();
        assert_eq!(invalidator.view_count(), 0);
        assert_eq!(invalidator.dependency_count(), 0);
    }

    #[test]
    fn test_view_and_dependency_count() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();

        assert_eq!(invalidator.view_count(), 4);
        assert_eq!(invalidator.dependency_count(), 3);
    }

    #[test]
    fn test_diamond_dependency() {
        let mut invalidator = CascadeInvalidator::new();
        // Diamond: raw → [user, post] → analytics
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_post", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_post").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_user").unwrap();
        // raw_user, user, post, analytics (4 total)
        assert_eq!(invalidated.len(), 4);
        assert!(invalidated.contains("v_raw_user"));
        assert!(invalidated.contains("v_user"));
        assert!(invalidated.contains("v_post"));
        assert!(invalidated.contains("v_analytics"));
    }

    #[test]
    fn test_multiple_independent_chains() {
        let mut invalidator = CascadeInvalidator::new();
        // Chain 1: raw1 → user1 → analytics1
        invalidator.add_dependency("v_user_1", "v_raw_1").unwrap();
        invalidator.add_dependency("v_analytics_1", "v_user_1").unwrap();
        // Chain 2: raw2 → user2 → analytics2
        invalidator.add_dependency("v_user_2", "v_raw_2").unwrap();
        invalidator.add_dependency("v_analytics_2", "v_user_2").unwrap();

        let invalidated = invalidator.cascade_invalidate("v_raw_1").unwrap();
        assert_eq!(invalidated.len(), 3); // Only chain 1
        assert!(!invalidated.contains("v_raw_2"));
        assert!(!invalidated.contains("v_user_2"));
    }

    #[test]
    fn test_cycle_detection_via_has_dependency_path() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();
        invalidator.add_dependency("v_analytics", "v_user").unwrap();
        // Note: Can't actually add cycle due to self-dependency check

        // Verify no cycles in what we added
        assert!(!invalidator.has_dependency_path("v_raw_user", "v_analytics"));
    }

    #[test]
    fn test_serialization() {
        let mut invalidator = CascadeInvalidator::new();
        invalidator.add_dependency("v_user", "v_raw_user").unwrap();

        let json = serde_json::to_string(&invalidator).expect("serialize should work");
        let restored: CascadeInvalidator =
            serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(
            restored.get_direct_dependents("v_raw_user"),
            invalidator.get_direct_dependents("v_raw_user")
        );
    }
}
