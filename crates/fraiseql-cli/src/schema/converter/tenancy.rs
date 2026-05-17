//! Compile-time `@tenant_id` validation for row-isolation tenancy.
//!
//! When `[fraiseql.tenancy] mode = "row"`, the compiler:
//! 1. Collects all types with fields annotated `@tenant_id`.
//! 2. For each query/mutation referencing such a type:
//!    - If `inject` is empty → auto-adds `{ <field>: jwt:<tenant_claim> }`.
//!    - If `inject` is non-empty but missing the annotated field → compile error.
//! 3. When no types have `@tenant_id` annotations → warning.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use tracing::warn;

use crate::schema::intermediate::{IntermediateSchema, IntermediateType};

/// Index of types annotated with `@tenant_id` and their annotated field names.
///
/// Built during compile-time validation. Maps type name → set of field names
/// carrying the `@tenant_id` directive.
#[derive(Debug, Default)]
pub struct AnnotatedTypeIndex {
    /// type_name → { field_name, ... }
    tenant_fields: HashMap<String, HashSet<String>>,
}

impl AnnotatedTypeIndex {
    /// Build the index from intermediate types.
    #[must_use]
    pub fn build(types: &[IntermediateType]) -> Self {
        let mut tenant_fields: HashMap<String, HashSet<String>> = HashMap::new();
        for typ in types {
            for field in &typ.fields {
                let has_tenant_id = field
                    .directives
                    .as_ref()
                    .is_some_and(|dirs| dirs.iter().any(|d| d.name == "tenant_id"));
                if has_tenant_id {
                    tenant_fields.entry(typ.name.clone()).or_default().insert(field.name.clone());
                }
            }
        }
        Self { tenant_fields }
    }

    /// Returns `true` if any type has `@tenant_id` annotations.
    #[must_use]
    pub fn has_annotations(&self) -> bool {
        !self.tenant_fields.is_empty()
    }

    /// Returns the set of `@tenant_id` field names for a given type, if any.
    #[must_use]
    pub fn fields_for_type(&self, type_name: &str) -> Option<&HashSet<String>> {
        self.tenant_fields.get(type_name)
    }
}

/// Validate and auto-inject `@tenant_id` parameters on queries and mutations.
///
/// When `mode = "row"`:
/// - Queries whose `return_type` is annotated get `inject` params auto-wired.
/// - Mutations whose `return_type` is annotated get `inject` params auto-wired.
/// - If `inject` is already non-empty but missing the tenant field → compile error.
///
/// When `mode = "schema"`, this function is a no-op.
///
/// # Errors
///
/// Returns an error if a query or mutation explicitly overrides `inject` without
/// including the `@tenant_id`-annotated field.
pub fn validate_tenant_annotations(
    schema: &mut IntermediateSchema,
    tenant_claim: &str,
) -> Result<()> {
    let index = AnnotatedTypeIndex::build(&schema.types);

    if !index.has_annotations() {
        warn!(
            "tenancy mode is 'row' but no types have @tenant_id annotations. \
             Add @tenant_id to fields that carry the tenant identifier."
        );
        return Ok(());
    }

    // Validate and auto-inject on queries
    for query in &mut schema.queries {
        if let Some(fields) = index.fields_for_type(&query.return_type) {
            for field_name in fields {
                let inject_source = format!("jwt:{tenant_claim}");
                if query.inject.is_empty() {
                    // Auto-inject: no explicit inject → safe to add
                    query.inject.insert(field_name.clone(), inject_source);
                } else if !query.inject.contains_key(field_name) {
                    // Explicit inject exists but missing tenant field → error
                    bail!(
                        "Query '{}' references @tenant_id-annotated type '{}' but \
                         lacks inject_params for '{}'. Add `inject.{} = \"{}\"` or \
                         remove the explicit inject to use auto-injection.",
                        query.name,
                        query.return_type,
                        field_name,
                        field_name,
                        inject_source,
                    );
                }
                // If inject already contains the field → ok, no action needed
            }
        }
    }

    // Validate and auto-inject on mutations
    for mutation in &mut schema.mutations {
        if let Some(fields) = index.fields_for_type(&mutation.return_type) {
            for field_name in fields {
                let inject_source = format!("jwt:{tenant_claim}");
                if mutation.inject.is_empty() {
                    mutation.inject.insert(field_name.clone(), inject_source);
                } else if !mutation.inject.contains_key(field_name) {
                    bail!(
                        "Mutation '{}' references @tenant_id-annotated type '{}' but \
                         lacks inject_params for '{}'. Add `inject.{} = \"{}\"` or \
                         remove the explicit inject to use auto-injection.",
                        mutation.name,
                        mutation.return_type,
                        field_name,
                        field_name,
                        inject_source,
                    );
                }
            }
        }
    }

    Ok(())
}
