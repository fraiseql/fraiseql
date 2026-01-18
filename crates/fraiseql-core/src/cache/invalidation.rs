//! Cache invalidation context and API.
//!
//! Provides structured invalidation contexts for different scenarios:
//! - Mutation-triggered invalidation
//! - Manual/administrative invalidation
//! - Schema change invalidation
//!
//! # Phase 2 Scope
//!
//! - View-based invalidation (not entity-level)
//! - Simple context types
//! - Structured logging support
//!
//! # Future Enhancements (Phase 7+)
//!
//! - Entity-level invalidation with cascade metadata
//! - Selective invalidation (by ID)
//! - Invalidation batching

/// Reason for cache invalidation.
///
/// Used for structured logging and debugging to understand why cache entries
/// were invalidated.
#[derive(Debug, Clone)]
pub enum InvalidationReason {
    /// Invalidation triggered by a GraphQL mutation.
    ///
    /// Contains the mutation name that modified the data.
    Mutation {
        /// Name of the mutation (e.g., "createUser", "updatePost")
        mutation_name: String,
    },

    /// Manual invalidation by administrator or system.
    ///
    /// Contains a custom reason string for audit logging.
    Manual {
        /// Human-readable reason (e.g., "maintenance", "data import")
        reason: String,
    },

    /// Schema change requiring cache flush.
    ///
    /// Triggered when the schema version changes (e.g., after deployment).
    SchemaChange {
        /// Old schema version
        old_version: String,
        /// New schema version
        new_version: String,
    },
}

impl InvalidationReason {
    /// Format reason as a log-friendly string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationReason;
    ///
    /// let reason = InvalidationReason::Mutation {
    ///     mutation_name: "createUser".to_string()
    /// };
    ///
    /// assert_eq!(
    ///     reason.to_log_string(),
    ///     "mutation:createUser"
    /// );
    /// ```
    #[must_use]
    pub fn to_log_string(&self) -> String {
        match self {
            Self::Mutation { mutation_name } => format!("mutation:{mutation_name}"),
            Self::Manual { reason } => format!("manual:{reason}"),
            Self::SchemaChange {
                old_version,
                new_version,
            } => {
                format!("schema_change:{old_version}->{new_version}")
            },
        }
    }
}

/// Context for cache invalidation operations.
///
/// Encapsulates which views/tables were modified and why, providing
/// structured information for logging and debugging.
///
/// # Example
///
/// ```rust
/// use fraiseql_core::cache::InvalidationContext;
///
/// // Invalidate after mutation
/// let ctx = InvalidationContext::for_mutation(
///     "createUser",
///     vec!["v_user".to_string()]
/// );
///
/// // Invalidate manually
/// let ctx = InvalidationContext::manual(
///     vec!["v_user".to_string(), "v_post".to_string()],
///     "data import completed"
/// );
///
/// // Invalidate on schema change
/// let ctx = InvalidationContext::schema_change(
///     vec!["v_user".to_string()],
///     "1.0.0",
///     "1.1.0"
/// );
/// ```
#[derive(Debug, Clone)]
pub struct InvalidationContext {
    /// List of views/tables that were modified.
    ///
    /// All cache entries accessing these views will be invalidated.
    pub modified_views: Vec<String>,

    /// Reason for invalidation.
    ///
    /// Used for structured logging and debugging.
    pub reason: InvalidationReason,
}

impl InvalidationContext {
    /// Create invalidation context for a mutation.
    ///
    /// Used by mutation handlers to invalidate cache entries after
    /// modifying data.
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - Name of the mutation (e.g., "createUser")
    /// * `modified_views` - List of views/tables modified by the mutation
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationContext;
    ///
    /// let ctx = InvalidationContext::for_mutation(
    ///     "createUser",
    ///     vec!["v_user".to_string()]
    /// );
    ///
    /// assert_eq!(ctx.modified_views, vec!["v_user"]);
    /// ```
    #[must_use]
    pub fn for_mutation(mutation_name: &str, modified_views: Vec<String>) -> Self {
        Self {
            modified_views,
            reason: InvalidationReason::Mutation {
                mutation_name: mutation_name.to_string(),
            },
        }
    }

    /// Create invalidation context for manual invalidation.
    ///
    /// Used by administrators or background jobs to manually invalidate
    /// cache entries (e.g., after data import, during maintenance).
    ///
    /// # Arguments
    ///
    /// * `modified_views` - List of views/tables to invalidate
    /// * `reason` - Human-readable reason for audit logging
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationContext;
    ///
    /// let ctx = InvalidationContext::manual(
    ///     vec!["v_user".to_string(), "v_post".to_string()],
    ///     "maintenance: rebuilding indexes"
    /// );
    ///
    /// assert_eq!(ctx.modified_views.len(), 2);
    /// ```
    #[must_use]
    pub fn manual(modified_views: Vec<String>, reason: &str) -> Self {
        Self {
            modified_views,
            reason: InvalidationReason::Manual {
                reason: reason.to_string(),
            },
        }
    }

    /// Create invalidation context for schema change.
    ///
    /// Used during deployments when the schema version changes to flush
    /// all cached entries that depend on the old schema structure.
    ///
    /// # Arguments
    ///
    /// * `affected_views` - All views in the schema (typically all views)
    /// * `old_version` - Previous schema version
    /// * `new_version` - New schema version
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationContext;
    ///
    /// let ctx = InvalidationContext::schema_change(
    ///     vec!["v_user".to_string(), "v_post".to_string()],
    ///     "1.0.0",
    ///     "1.1.0"
    /// );
    ///
    /// assert_eq!(ctx.modified_views.len(), 2);
    /// ```
    #[must_use]
    pub fn schema_change(
        affected_views: Vec<String>,
        old_version: &str,
        new_version: &str,
    ) -> Self {
        Self {
            modified_views: affected_views,
            reason:         InvalidationReason::SchemaChange {
                old_version: old_version.to_string(),
                new_version: new_version.to_string(),
            },
        }
    }

    /// Get a log-friendly description of this invalidation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationContext;
    ///
    /// let ctx = InvalidationContext::for_mutation(
    ///     "createUser",
    ///     vec!["v_user".to_string()]
    /// );
    ///
    /// assert_eq!(
    ///     ctx.to_log_string(),
    ///     "mutation:createUser affecting 1 view(s)"
    /// );
    /// ```
    #[must_use]
    pub fn to_log_string(&self) -> String {
        format!(
            "{} affecting {} view(s)",
            self.reason.to_log_string(),
            self.modified_views.len()
        )
    }

    /// Get the number of views affected by this invalidation.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationContext;
    ///
    /// let ctx = InvalidationContext::manual(
    ///     vec!["v_user".to_string(), "v_post".to_string()],
    ///     "maintenance"
    /// );
    ///
    /// assert_eq!(ctx.view_count(), 2);
    /// ```
    #[must_use]
    pub fn view_count(&self) -> usize {
        self.modified_views.len()
    }

    /// Check if this invalidation affects a specific view.
    ///
    /// # Arguments
    ///
    /// * `view` - View name to check
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::InvalidationContext;
    ///
    /// let ctx = InvalidationContext::for_mutation(
    ///     "createUser",
    ///     vec!["v_user".to_string(), "v_post".to_string()]
    /// );
    ///
    /// assert!(ctx.affects_view("v_user"));
    /// assert!(ctx.affects_view("v_post"));
    /// assert!(!ctx.affects_view("v_comment"));
    /// ```
    #[must_use]
    pub fn affects_view(&self, view: &str) -> bool {
        self.modified_views.iter().any(|v| v == view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_for_mutation() {
        let ctx = InvalidationContext::for_mutation("createUser", vec!["v_user".to_string()]);

        assert_eq!(ctx.modified_views, vec!["v_user"]);
        assert!(matches!(ctx.reason, InvalidationReason::Mutation { .. }));
    }

    #[test]
    fn test_manual() {
        let ctx = InvalidationContext::manual(
            vec!["v_user".to_string(), "v_post".to_string()],
            "maintenance",
        );

        assert_eq!(ctx.modified_views.len(), 2);
        assert!(matches!(ctx.reason, InvalidationReason::Manual { .. }));
    }

    #[test]
    fn test_schema_change() {
        let ctx = InvalidationContext::schema_change(vec!["v_user".to_string()], "1.0.0", "1.1.0");

        assert_eq!(ctx.modified_views, vec!["v_user"]);
        assert!(matches!(ctx.reason, InvalidationReason::SchemaChange { .. }));
    }

    #[test]
    fn test_mutation_log_string() {
        let ctx = InvalidationContext::for_mutation("createUser", vec!["v_user".to_string()]);

        assert_eq!(ctx.to_log_string(), "mutation:createUser affecting 1 view(s)");
    }

    #[test]
    fn test_manual_log_string() {
        let ctx = InvalidationContext::manual(vec!["v_user".to_string()], "data import");

        assert_eq!(ctx.to_log_string(), "manual:data import affecting 1 view(s)");
    }

    #[test]
    fn test_schema_change_log_string() {
        let ctx = InvalidationContext::schema_change(vec!["v_user".to_string()], "1.0.0", "1.1.0");

        assert_eq!(ctx.to_log_string(), "schema_change:1.0.0->1.1.0 affecting 1 view(s)");
    }

    #[test]
    fn test_view_count() {
        let ctx = InvalidationContext::for_mutation(
            "createUser",
            vec!["v_user".to_string(), "v_post".to_string()],
        );

        assert_eq!(ctx.view_count(), 2);
    }

    #[test]
    fn test_affects_view() {
        let ctx = InvalidationContext::for_mutation(
            "createUser",
            vec!["v_user".to_string(), "v_post".to_string()],
        );

        assert!(ctx.affects_view("v_user"));
        assert!(ctx.affects_view("v_post"));
        assert!(!ctx.affects_view("v_comment"));
    }

    #[test]
    fn test_empty_views() {
        let ctx = InvalidationContext::manual(vec![], "testing empty invalidation");

        assert_eq!(ctx.view_count(), 0);
        assert!(!ctx.affects_view("v_user"));
    }

    #[test]
    fn test_reason_to_log_string_mutation() {
        let reason = InvalidationReason::Mutation {
            mutation_name: "updatePost".to_string(),
        };

        assert_eq!(reason.to_log_string(), "mutation:updatePost");
    }

    #[test]
    fn test_reason_to_log_string_manual() {
        let reason = InvalidationReason::Manual {
            reason: "cache warmup".to_string(),
        };

        assert_eq!(reason.to_log_string(), "manual:cache warmup");
    }

    #[test]
    fn test_reason_to_log_string_schema_change() {
        let reason = InvalidationReason::SchemaChange {
            old_version: "2.0.0".to_string(),
            new_version: "2.1.0".to_string(),
        };

        assert_eq!(reason.to_log_string(), "schema_change:2.0.0->2.1.0");
    }

    #[test]
    fn test_multiple_views() {
        let views = vec![
            "v_user".to_string(),
            "v_post".to_string(),
            "v_comment".to_string(),
            "v_like".to_string(),
        ];

        let ctx = InvalidationContext::for_mutation("deleteUser", views);

        assert_eq!(ctx.view_count(), 4);
        assert!(ctx.affects_view("v_user"));
        assert!(ctx.affects_view("v_post"));
        assert!(ctx.affects_view("v_comment"));
        assert!(ctx.affects_view("v_like"));
        assert!(!ctx.affects_view("v_notification"));
    }
}
