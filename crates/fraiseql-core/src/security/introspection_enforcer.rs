//! Introspection Enforcer
//!
//! This module provides control over GraphQL introspection queries.
//! It enforces policies that control whether clients can query the schema.
//!
//! # Architecture
//!
//! The enforcer is invoked from the server's GraphQL request handler (after the
//! query string is resolved, before execution) to gate `{ __schema }` /
//! `{ __type }` on the configured [`IntrospectionPolicy`]. The policy is derived
//! from the server's `introspection_enabled` / `introspection_require_auth`
//! settings via [`IntrospectionPolicy::from_config`], the same derivation the
//! REST `/introspection` endpoint uses.
//!
//! ```text
//! GraphQL Query
//!     ↓
//! IntrospectionEnforcer::validate_query()
//!     ├─ Check 1: AST-detect a root `__schema` / `__type` field (never `__typename`)
//!     ├─ Check 2: Check user authentication (for `InternalOnly`)
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

use crate::{
    graphql::{ParsedQuery, parse_query},
    security::errors::{Result, SecurityError},
};

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

impl IntrospectionPolicy {
    /// Derive the policy from the server's two introspection config booleans.
    ///
    /// This is the single source of truth shared by the GraphQL request-path
    /// gate (via `AppState::introspection_policy`) and the REST
    /// `/introspection` endpoint mount decision, so the two agree by
    /// construction:
    ///
    /// | `enabled` | `require_auth` | policy |
    /// |-----------|----------------|--------|
    /// | `false`   | (any)          | [`Disabled`](Self::Disabled) |
    /// | `true`    | `true`         | [`InternalOnly`](Self::InternalOnly) |
    /// | `true`    | `false`        | [`Allowed`](Self::Allowed) |
    ///
    /// The `enabled == false` row maps to `Disabled`, matching the
    /// fail-closed server default (introspection off unless explicitly enabled).
    #[must_use]
    pub const fn from_config(enabled: bool, require_auth: bool) -> Self {
        match (enabled, require_auth) {
            (false, _) => Self::Disabled,
            (true, true) => Self::InternalOnly,
            (true, false) => Self::Allowed,
        }
    }
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

/// Introspection detection configuration.
///
/// Selects which GraphQL introspection meta-fields a policy treats as
/// introspection. Only the two spec-defined, policy-controllable meta-fields
/// are configurable:
///
/// - `__schema` — the schema introspection root field.
/// - `__type` — the single-type introspection root field.
///
/// `__typename` is intentionally **not** configurable: the GraphQL spec
/// (§"Type Name Introspection") makes it queryable on every type regardless of
/// introspection policy, so blocking it would be non-conformant. There is also
/// no `__directive` meta-field in GraphQL — directive metadata is reached via
/// `__schema.directives` — so it is not a separate detection target.
#[derive(Debug, Clone)]
pub struct IntrospectionConfig {
    /// Whether to treat a root `__schema` field as introspection.
    pub detect_schema: bool,

    /// Whether to treat a root `__type` field as introspection.
    pub detect_type: bool,
}

impl IntrospectionConfig {
    /// Create a configuration that detects every controllable introspection
    /// meta-field (`__schema` and `__type`).
    #[must_use]
    pub const fn all() -> Self {
        Self {
            detect_schema: true,
            detect_type:   true,
        }
    }

    /// Create a configuration that detects the introspection meta-fields.
    ///
    /// Since #454 removed the non-conformant `__typename` knob and the
    /// non-existent `__directive` knob, `strict` and [`all`](Self::all) detect
    /// the same set; both are retained for API compatibility.
    #[must_use]
    pub const fn strict() -> Self {
        Self::all()
    }
}

/// Introspection Enforcer
///
/// Controls and enforces policies around GraphQL introspection queries.
/// Invoked from the GraphQL request handler to block `{ __schema }` /
/// `{ __type }` according to the configured [`IntrospectionPolicy`].
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

    /// Check if a query is an introspection query.
    ///
    /// Detection is **AST-based**: the query is parsed and the operation's root
    /// selection set is scanned for the introspection meta-fields `__schema` /
    /// `__type`, matched on the field *name* (not its response alias). This is
    /// the same source of truth the executor's classifier uses, so it shares
    /// the classifier's guarantees:
    ///
    /// - Aliased introspection is still caught (`{ x: __schema { .. } }`).
    /// - String argument values and comments do not false-positive (`{ search(q: "__schema") }`, `#
    ///   mentions __type`).
    /// - `__typename` is **never** treated as introspection, at any depth.
    /// - Multi-root documents are scanned in full, so `{ users { id } __schema { .. } }` is
    ///   detected.
    ///
    /// A query that fails to parse is reported as *not* introspection: the
    /// malformed input is rejected later on the normal parse path in the
    /// executor/handler, and the enforcer must not fail-open into a 500 or
    /// double-report the error.
    pub(crate) fn is_introspection_query(&self, query: &str) -> bool {
        match parse_query(query) {
            Ok(parsed) => self.is_introspection(&parsed),
            Err(_) => false,
        }
    }

    /// Scan an already-parsed query's root selection set for the configured
    /// introspection meta-fields.
    ///
    /// Borrows the AST so a caller that has already parsed the query (the
    /// GraphQL request handler) can avoid a redundant parse. Matches on
    /// [`FieldSelection::name`](crate::graphql::FieldSelection::name) so
    /// aliases are transparent and never matches `__typename`.
    #[must_use]
    pub fn is_introspection(&self, parsed: &ParsedQuery) -> bool {
        parsed.selections.iter().any(|sel| {
            (self.config.detect_schema && sel.name == "__schema")
                || (self.config.detect_type && sel.name == "__type")
        })
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
