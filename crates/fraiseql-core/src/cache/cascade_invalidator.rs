//! Cascading cache invalidation for transitive view dependencies.
//!
//! Tracks view-to-view dependencies and propagates invalidation through the dependency graph.
//! When a view is invalidated, all dependent views are also invalidated.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

#[allow(unused_imports)] // Reason: used only in doc links for `# Errors` sections
use crate::error::FraiseQLError;
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
    /// Example: `v_user` → [`v_raw_user`]
    view_dependencies: HashMap<String, HashSet<String>>,

    /// View → list of views that depend on it (reverse mapping).
    /// Example: `v_raw_user` → [`v_user`]
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
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if `dependent_view` and
    /// `dependency_view` are the same (self-dependency), or if adding the edge
    /// would create a cycle in the dependency graph.
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

        // Check for indirect cycles: would adding dependent → dependency create a cycle?
        // That happens if dependency_view can already reach dependent_view via existing edges.
        // We traverse the `dependents` graph (reverse edges) from dependency_view and check
        // whether we ever reach dependent_view.
        if self.can_reach(dependency_view, dependent_view) {
            return Err(crate::error::FraiseQLError::Validation {
                message: format!(
                    "Adding dependency '{}' → '{}' would create a cycle",
                    dependent_view, dependency_view
                ),
                path:    Some("cascade_invalidator::add_dependency".to_string()),
            });
        }

        let dependent = dependent_view.to_string();
        let dependency = dependency_view.to_string();

        // Add forward mapping: dependent → dependency
        self.view_dependencies
            .entry(dependent.clone())
            .or_default()
            .insert(dependency.clone());

        // Add reverse mapping: dependency → dependent
        self.dependents.entry(dependency).or_default().insert(dependent);

        Ok(())
    }

    /// Check whether `from` can reach `target` by following the forward dependency graph.
    ///
    /// Uses BFS over `view_dependencies` (from dependent to what it depends on). Returns
    /// `true` if `target` is reachable from `from`, meaning `from` transitively depends on
    /// `target`.
    ///
    /// This is used to detect cycles before adding a new edge: if `dependency_view` can
    /// already reach `dependent_view`, adding `dependent_view → dependency_view` would
    /// create a cycle.
    fn can_reach(&self, from: &str, target: &str) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == target {
                return true;
            }
            if !visited.insert(current.clone()) {
                continue; // already visited
            }
            // Follow forward edges: what does `current` depend on?
            if let Some(deps) = self.view_dependencies.get(&current) {
                for dep in deps {
                    if !visited.contains(dep.as_str()) {
                        queue.push_back(dep.clone());
                    }
                }
            }
        }

        false
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
    /// # Errors
    ///
    /// Currently infallible — always returns `Ok`. The `Result` return type is
    /// reserved for future cycle-detection logic that may return
    /// [`FraiseQLError::Validation`] on circular dependency graphs.
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
        #[allow(clippy::cast_possible_truncation)]
        // Reason: invalidated.len() is a usize which fits in u64 on all supported 64-bit platforms
        {
            self.stats.total_invalidated += invalidated.len() as u64;
        }
        self.stats.max_affected = self.stats.max_affected.max(invalidated.len());
        if self.stats.total_cascades > 0 {
            #[allow(clippy::cast_precision_loss)]
            // Reason: average_affected is a display metric; f64 precision loss on u64 counters is
            // acceptable
            {
                self.stats.average_affected =
                    self.stats.total_invalidated as f64 / self.stats.total_cascades as f64;
            }
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn view_count(&self) -> usize {
        let mut views = HashSet::new();
        views.extend(self.dependents.keys().cloned());
        views.extend(self.view_dependencies.keys().cloned());
        views.len()
    }

    /// Get total number of dependency edges.
    #[must_use]
    pub fn dependency_count(&self) -> usize {
        self.view_dependencies.values().map(|deps| deps.len()).sum()
    }
}

impl Default for CascadeInvalidator {
    fn default() -> Self {
        Self::new()
    }
}
