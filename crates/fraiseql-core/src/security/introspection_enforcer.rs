//! Introspection Enforcer
//!
//! This module provides control over GraphQL introspection queries.
//! It enforces policies that control whether clients can query the schema.
//!
//! # Architecture
//!
//! The Introspection Enforcer acts as the fourth layer in the security middleware:
//! ```text
//! GraphQL Query
//!     ↓
//! IntrospectionEnforcer::validate_query()
//!     ├─ Check 1: Detect introspection patterns (__schema, __type)
//!     ├─ Check 2: Check user authentication (if required)
//!     └─ Check 3: Apply policy enforcement
//!     ↓
//! Result<()> (query allowed or blocked)
//! ```
//!
//! # Examples
//!
//! ```no_run
//! use fraiseql_core::security::{IntrospectionEnforcer, IntrospectionPolicy};
//!
//! // Create enforcer that disables introspection
//! let enforcer = IntrospectionEnforcer::new(IntrospectionPolicy::Disabled);
//!
//! // Check if a query is introspection
//! let introspection_query = "{ __schema { types { name } } }";
//! match enforcer.validate_query(introspection_query, None) {
//!     Err(e) => println!("Introspection not allowed: {}", e),
//!     Ok(_) => println!("Query allowed"),
//! }
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::security::errors::{Result, SecurityError};

/// Introspection policy for controlling schema access
///
/// Defines what level of introspection is allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IntrospectionPolicy {
    /// Introspection queries are allowed for all clients
    Allowed,

    /// Introspection queries are completely disabled
    Disabled,

    /// Introspection queries are allowed only for authenticated users
    InternalOnly,
}

impl fmt::Display for IntrospectionPolicy {
    #[cfg_attr(test, mutants::skip)]
    // Reason: diagnostic Display impl — string values are not asserted by any test;
    // mutations to the variant strings cannot be killed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allowed => write!(f, "Allowed"),
            Self::Disabled => write!(f, "Disabled"),
            Self::InternalOnly => write!(f, "InternalOnly"),
        }
    }
}

/// Introspection detection configuration
///
/// Contains patterns and configuration for detecting introspection queries.
#[derive(Debug, Clone)]
pub struct IntrospectionConfig {
    /// Whether to detect __schema queries
    pub detect_schema: bool,

    /// Whether to detect __type queries
    pub detect_type: bool,

    /// Whether to detect __typename queries
    pub detect_typename: bool,

    /// Whether to detect __directive queries
    pub detect_directive: bool,
}

impl IntrospectionConfig {
    /// Create a configuration that detects all introspection patterns
    #[must_use]
    pub const fn all() -> Self {
        Self {
            detect_schema:    true,
            detect_type:      true,
            detect_typename:  true,
            detect_directive: true,
        }
    }

    /// Create a configuration that detects only the main introspection patterns
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            detect_schema:    true,
            detect_type:      true,
            detect_typename:  false,
            detect_directive: true,
        }
    }
}

/// Introspection Enforcer
///
/// Controls and enforces policies around GraphQL introspection queries.
/// Acts as the fourth layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct IntrospectionEnforcer {
    policy: IntrospectionPolicy,
    config: IntrospectionConfig,
}

impl IntrospectionEnforcer {
    /// Create a new introspection enforcer with a specific policy
    #[must_use]
    pub const fn new(policy: IntrospectionPolicy) -> Self {
        Self {
            policy,
            config: IntrospectionConfig::all(),
        }
    }

    /// Create enforcer with custom configuration
    #[must_use]
    pub const fn with_config(policy: IntrospectionPolicy, config: IntrospectionConfig) -> Self {
        Self { policy, config }
    }

    /// Create enforcer with Allowed policy (standard)
    #[must_use]
    pub const fn allowed() -> Self {
        Self::new(IntrospectionPolicy::Allowed)
    }

    /// Create enforcer with Disabled policy (strict)
    #[must_use]
    pub const fn disabled() -> Self {
        Self::new(IntrospectionPolicy::Disabled)
    }

    /// Create enforcer with `InternalOnly` policy (regulated)
    #[must_use]
    pub const fn internal_only() -> Self {
        Self::new(IntrospectionPolicy::InternalOnly)
    }

    /// Validate whether an introspection query is allowed.
    ///
    /// Performs 3 validation checks:
    /// 1. Detect if query is an introspection query
    /// 2. Check user authentication (if required by policy)
    /// 3. Apply policy enforcement
    ///
    /// # Arguments
    /// * `query` - The GraphQL query string to validate
    /// * `authenticated_user_id` - Optional user ID (None = anonymous, Some(id) = authenticated)
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError::IntrospectionDisabled`] if the query is an
    /// introspection query and the active policy disallows it (either globally
    /// disabled or requiring authentication when the user is anonymous).
    pub fn validate_query(&self, query: &str, authenticated_user_id: Option<&str>) -> Result<()> {
        // Check 1: Detect introspection patterns
        let is_introspection = self.is_introspection_query(query);

        // If not introspection, allow regardless of policy
        if !is_introspection {
            return Ok(());
        }

        // Check 2 & 3: Apply policy
        match self.policy {
            IntrospectionPolicy::Allowed => {
                // Introspection allowed for all
                Ok(())
            },
            IntrospectionPolicy::Disabled => {
                // Introspection disabled for everyone
                Err(SecurityError::IntrospectionDisabled {
                    detail: "Introspection queries are disabled in this environment".to_string(),
                })
            },
            IntrospectionPolicy::InternalOnly => {
                // Introspection allowed only for authenticated users
                if authenticated_user_id.is_some() {
                    Ok(())
                } else {
                    Err(SecurityError::IntrospectionDisabled {
                        detail: "Introspection queries require authentication".to_string(),
                    })
                }
            },
        }
    }

    /// Check if a query is an introspection query
    ///
    /// Detects GraphQL introspection patterns:
    /// - `__schema`
    /// - `__type`
    /// - `__typename` (if enabled)
    /// - `__directive` (if enabled)
    pub(crate) fn is_introspection_query(&self, query: &str) -> bool {
        let query_lower = query.to_lowercase();

        // Check for __schema
        if self.config.detect_schema && query_lower.contains("__schema") {
            return true;
        }

        // Check for __type
        if self.config.detect_type && query_lower.contains("__type") {
            return true;
        }

        // Check for __typename
        if self.config.detect_typename && query_lower.contains("__typename") {
            return true;
        }

        // Check for __directive
        if self.config.detect_directive && query_lower.contains("__directive") {
            return true;
        }

        false
    }

    /// Get the current policy
    #[must_use]
    pub const fn policy(&self) -> IntrospectionPolicy {
        self.policy
    }

    /// Get the detection configuration
    #[must_use]
    pub const fn config(&self) -> &IntrospectionConfig {
        &self.config
    }
}
