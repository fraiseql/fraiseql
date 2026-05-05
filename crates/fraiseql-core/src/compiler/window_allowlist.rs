//! Schema-based allowlist for window query identifiers.
//!
//! Provides defence-in-depth on top of the character-level validation in
//! `window_functions/planner.rs`: every identifier used in a window query's
//! PARTITION BY, ORDER BY, or SELECT clauses is checked against the set of
//! fields declared in the compiled schema.
//!
//! When the allowlist is empty (i.e. the schema declares no fact-table metadata
//! for this type) the validation is skipped, matching the behaviour of
//! `compiler/aggregation.rs` when `metadata.dimensions.paths` is empty.

use std::collections::HashSet;

use crate::{
    compiler::fact_table::FactTableMetadata,
    error::{FraiseQLError, Result},
};

/// Validated allowlist of identifiers permitted in window queries for a given type.
///
/// Built at request-planning time from the compiled schema's `FactTableMetadata`.
/// An empty allowlist means "no schema constraints are declared; character-level
/// validation still applies".
#[derive(Debug, Clone, Default)]
pub struct WindowAllowlist {
    /// All valid field expressions.
    ///
    /// Contains:
    /// - measure column names (e.g. `"revenue"`)
    /// - denormalised filter column names (e.g. `"occurred_at"`)
    /// - dimension JSONB path expressions (e.g. `"dimensions->>'category'"`)
    fields: HashSet<String>,
}

impl WindowAllowlist {
    /// Build an allowlist from compiled fact-table metadata.
    #[must_use]
    pub fn from_metadata(metadata: &FactTableMetadata) -> Self {
        let mut fields = HashSet::new();
        for m in &metadata.measures {
            fields.insert(m.name.clone());
        }
        for f in &metadata.denormalized_filters {
            fields.insert(f.name.clone());
        }
        for p in &metadata.dimensions.paths {
            // Store both the short name ("category") and the full JSONB expression
            // ("dimensions->>'category'") so callers can use either form.
            fields.insert(p.name.clone());
            fields.insert(p.json_path.clone());
        }
        Self { fields }
    }

    /// Returns `true` if no schema constraints are declared.
    ///
    /// An empty allowlist does not block any identifier; character-level
    /// validation in the planner still applies.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Validate that `identifier` is in the allowlist.
    ///
    /// When the allowlist is empty (no schema constraints), this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the identifier is not in the
    /// allowlist and the allowlist is non-empty.
    pub fn validate(&self, identifier: &str, context: &str) -> Result<()> {
        if self.fields.is_empty() || self.fields.contains(identifier) {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Field '{identifier}' is not a known {context} field for this window query. \
                     Only fields declared in the compiled schema are permitted."
                ),
                path:    None,
            })
        }
    }
}
