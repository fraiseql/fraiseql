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
                let has_tenant_id = field.directives.as_ref().is_some_and(|dirs| {
                    dirs.iter().any(|d| d.name == "tenant_id")
                });
                if has_tenant_id {
                    tenant_fields
                        .entry(typ.name.clone())
                        .or_default()
                        .insert(field.name.clone());
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
                    query
                        .inject
                        .insert(field_name.clone(), inject_source);
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
                    mutation
                        .inject
                        .insert(field_name.clone(), inject_source);
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions

    use indexmap::IndexMap;

    use super::*;
    use crate::schema::intermediate::{
        IntermediateField, IntermediateMutation, IntermediateQuery, IntermediateType,
        fragments::IntermediateAppliedDirective,
    };

    fn make_type(name: &str, fields: Vec<IntermediateField>) -> IntermediateType {
        IntermediateType {
            name: name.to_string(),
            fields,
            description: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
        }
    }

    fn make_field(name: &str, field_type: &str) -> IntermediateField {
        IntermediateField {
            name:           name.to_string(),
            field_type:     field_type.to_string(),
            nullable:       false,
            description:    None,
            directives:     None,
            requires_scope: None,
            on_deny:        None,
        }
    }

    fn make_tenant_id_field(name: &str) -> IntermediateField {
        IntermediateField {
            name:           name.to_string(),
            field_type:     "String".to_string(),
            nullable:       false,
            description:    None,
            directives:     Some(vec![IntermediateAppliedDirective {
                name:      "tenant_id".to_string(),
                arguments: None,
            }]),
            requires_scope: None,
            on_deny:        None,
        }
    }

    fn make_query(name: &str, return_type: &str) -> IntermediateQuery {
        IntermediateQuery {
            name:        name.to_string(),
            return_type: return_type.to_string(),
            ..Default::default()
        }
    }

    fn make_mutation(name: &str, return_type: &str) -> IntermediateMutation {
        IntermediateMutation {
            name:        name.to_string(),
            return_type: return_type.to_string(),
            ..Default::default()
        }
    }

    fn make_schema(
        types: Vec<IntermediateType>,
        queries: Vec<IntermediateQuery>,
        mutations: Vec<IntermediateMutation>,
    ) -> IntermediateSchema {
        IntermediateSchema {
            types,
            queries,
            mutations,
            ..Default::default()
        }
    }

    // ── AnnotatedTypeIndex ──────────────────────────────────────────────

    #[test]
    fn index_empty_when_no_annotations() {
        let types = vec![make_type("User", vec![make_field("id", "Int")])];
        let index = AnnotatedTypeIndex::build(&types);
        assert!(!index.has_annotations());
    }

    #[test]
    fn index_detects_tenant_id_field() {
        let types = vec![make_type(
            "User",
            vec![make_field("id", "Int"), make_tenant_id_field("tenant_id")],
        )];
        let index = AnnotatedTypeIndex::build(&types);
        assert!(index.has_annotations());
        let fields = index.fields_for_type("User").unwrap();
        assert!(fields.contains("tenant_id"));
    }

    #[test]
    fn index_multiple_types_independently() {
        let types = vec![
            make_type("User", vec![make_tenant_id_field("tenant_id")]),
            make_type("Post", vec![make_field("id", "Int")]),
            make_type("Order", vec![make_tenant_id_field("org_id")]),
        ];
        let index = AnnotatedTypeIndex::build(&types);
        assert!(index.fields_for_type("User").is_some());
        assert!(index.fields_for_type("Post").is_none());
        assert!(index.fields_for_type("Order").is_some());
        assert!(index.fields_for_type("Order").unwrap().contains("org_id"));
    }

    // ── Auto-injection ──────────────────────────────────────────────────

    #[test]
    fn auto_injects_query_when_inject_empty() {
        let mut schema = make_schema(
            vec![make_type(
                "User",
                vec![make_field("id", "Int"), make_tenant_id_field("tenant_id")],
            )],
            vec![make_query("getUser", "User")],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
        assert_eq!(
            schema.queries[0].inject.get("tenant_id"),
            Some(&"jwt:tenant_id".to_string())
        );
    }

    #[test]
    fn auto_injects_mutation_when_inject_empty() {
        let mut schema = make_schema(
            vec![make_type(
                "User",
                vec![make_field("id", "Int"), make_tenant_id_field("tenant_id")],
            )],
            vec![],
            vec![make_mutation("createUser", "User")],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
        assert_eq!(
            schema.mutations[0].inject.get("tenant_id"),
            Some(&"jwt:tenant_id".to_string())
        );
    }

    #[test]
    fn auto_inject_uses_custom_claim() {
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![make_query("getUser", "User")],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "org_id").unwrap();
        assert_eq!(
            schema.queries[0].inject.get("tenant_id"),
            Some(&"jwt:org_id".to_string())
        );
    }

    // ── Existing inject accepted ────────────────────────────────────────

    #[test]
    fn existing_inject_with_tenant_field_accepted() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), "jwt:tenant_id".to_string());
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![IntermediateQuery {
                name:        "getUser".to_string(),
                return_type: "User".to_string(),
                inject,
                ..Default::default()
            }],
            vec![],
        );
        // Should succeed — inject already has the tenant field
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
    }

    // ── Error: explicit inject missing tenant ───────────────────────────

    #[test]
    fn error_when_inject_overridden_without_tenant() {
        let mut inject = IndexMap::new();
        inject.insert("user_id".to_string(), "jwt:sub".to_string());
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![IntermediateQuery {
                name:        "getUser".to_string(),
                return_type: "User".to_string(),
                inject,
                ..Default::default()
            }],
            vec![],
        );
        let err = validate_tenant_annotations(&mut schema, "tenant_id").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("getUser"), "error should mention query name: {msg}");
        assert!(msg.contains("@tenant_id"), "error should mention directive: {msg}");
        assert!(msg.contains("tenant_id"), "error should mention field: {msg}");
    }

    #[test]
    fn error_mutation_inject_overridden_without_tenant() {
        let mut inject = IndexMap::new();
        inject.insert("user_id".to_string(), "jwt:sub".to_string());
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![],
            vec![IntermediateMutation {
                name:        "createUser".to_string(),
                return_type: "User".to_string(),
                inject,
                ..Default::default()
            }],
        );
        let err = validate_tenant_annotations(&mut schema, "tenant_id").unwrap_err();
        assert!(err.to_string().contains("createUser"));
    }

    // ── No-op for non-annotated types ───────────────────────────────────

    #[test]
    fn query_on_non_annotated_type_unchanged() {
        let mut schema = make_schema(
            vec![make_type("Post", vec![make_field("id", "Int")])],
            vec![make_query("getPosts", "Post")],
            vec![],
        );
        // Warning about no annotations, but no error
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
        assert!(schema.queries[0].inject.is_empty());
    }

    // ── Warning when no annotations ─────────────────────────────────────

    #[test]
    fn warning_when_no_tenant_id_annotations() {
        let mut schema = make_schema(
            vec![make_type("User", vec![make_field("id", "Int")])],
            vec![make_query("getUser", "User")],
            vec![],
        );
        // Should succeed with a warning (no error)
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
    }

    // ── schema mode skips validation entirely ───────────────────────────
    // (This is handled by the caller in compile.rs, not by this function)
}
