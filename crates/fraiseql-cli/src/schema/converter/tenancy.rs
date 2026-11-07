//! Compile-time `@tenant_id` validation for row-isolation tenancy.
//!
//! When `[tenancy] mode = "row"`, the compiler:
//! 1. Collects all types with fields annotated `@tenant_id`.
//! 2. For each query/mutation referencing such a type:
//!    - If `inject` is empty → auto-adds `{ <field>: jwt:<tenant_claim> }`.
//!    - If `inject` is non-empty but missing the annotated field → compile error.
//! 3. For each annotated type **no query returns** — a federation entity reached only through
//!    `_entities`, which has no operation to lower the annotation onto (#1142) — the same two rules
//!    applied to the type's own `inject_params`.
//! 4. When no type has a `@tenant_id` annotation → compile error (#1386): row isolation configured
//!    to scope nothing is a misconfiguration, not a note.
//! 5. Every type a root returns — a query, mutation or subscription return type, through union
//!    members and interface implementors, and every federation entity no query returns — carries a
//!    `@tenant_id` field or is listed in `[tenancy] shared_types`. A mutation that passes the
//!    tenant claim to its function is scoped by it. Anything else is a compile error: an
//!    unannotated type is either shared or a per-tenant type its author forgot to annotate, and
//!    only the author can say which.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};

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
/// Returns an error if no type carries a `@tenant_id` annotation, if a query or mutation
/// explicitly sets `inject` without including the annotated field, or if a type a root
/// returns is neither annotated nor listed in `shared_types` (or is both).
pub fn validate_tenant_annotations(
    schema: &mut IntermediateSchema,
    tenant_claim: &str,
    shared_types: &[String],
) -> Result<()> {
    // #1418: an optional claim passes NULL when the token lacks it, and a function must
    // never receive a NULL tenant. Checked on every operation, annotated or not.
    let optional_tenant = format!("jwt_optional:{tenant_claim}");
    let operations = schema
        .queries
        .iter()
        .map(|q| ("Query", &q.name, &q.inject))
        .chain(schema.mutations.iter().map(|m| ("Mutation", &m.name, &m.inject)));
    for (kind, name, inject) in operations {
        if let Some((param, _)) = inject.iter().find(|(_, source)| **source == optional_tenant) {
            bail!(
                "{kind} '{name}': inject param '{param}' makes the tenant claim \
                 '{tenant_claim}' optional. The tenant claim is never optional; use \
                 'jwt:{tenant_claim}'."
            );
        }
    }

    let index = AnnotatedTypeIndex::build(&schema.types);

    // #1386: a warning here read as "configured, with a note", while every query stayed
    // unscoped and the artifact was byte-identical to the unconfigured one. Row isolation
    // that matches nothing is a security control configured to do nothing; say so, with
    // the scope of the no-op, and name the setting for isolation enforced elsewhere.
    if !index.has_annotations() {
        bail!(
            "[tenancy] mode = \"row\" matched 0 of {} types: no field carries the \
             @tenant_id directive, so no query would be scoped to the caller's tenant. Mark \
             the field that holds the tenant on each per-tenant type with @tenant_id. If row \
             isolation is enforced by database RLS instead, remove `mode = \"row\"` and \
             declare `[security] multi_tenant = true` with \
             `[security.rls] enabled = true`.",
            schema.types.len()
        );
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

    // Validate and auto-inject on mutations.
    //
    // A mutation's inject key is a *function parameter* name, not a column: the annotation
    // says which field of the returned rows holds the tenant, which says nothing about how
    // the writer receives it. So a mutation that already passes the tenant claim — under
    // any parameter name — is scoped, and adding a second argument for the annotated field
    // would only break its function's arity (#1386). One that passes it nowhere is refused.
    for mutation in &mut schema.mutations {
        if let Some(fields) = index.fields_for_type(&mutation.return_type) {
            let inject_source = format!("jwt:{tenant_claim}");
            if mutation.inject.values().any(|source| *source == inject_source) {
                continue;
            }
            for field_name in fields {
                if mutation.inject.is_empty() {
                    mutation.inject.insert(field_name.clone(), inject_source.clone());
                } else if !mutation.inject.contains_key(field_name) {
                    bail!(
                        "Mutation '{}' returns @tenant_id-annotated type '{}' but passes the \
                         tenant to its function under no parameter: none of its inject_params \
                         reads '{}'. Add one (for example `inject.{} = \"{}\"`, named after the \
                         function's tenant parameter), or remove the explicit inject to use \
                         auto-injection.",
                        mutation.name,
                        mutation.return_type,
                        inject_source,
                        field_name,
                        inject_source,
                    );
                }
            }
        }
    }

    // #1142: a type that no *query* returns has no operation for the annotation to be
    // lowered onto. The federation `_entities` path reads such a type's own
    // `inject_params` instead, so auto-inject there on the same terms — a `@tenant_id`
    // annotation then means the same thing for an entity resolved through federation as
    // for one behind a query.
    //
    // Deliberately restricted to the queryless shape: `_entities` consults the backing
    // query first, so a type a query returns is already scoped, and injecting anyway
    // would grow every tenancy-annotated schema a redundant second copy of each
    // declaration. Mutations do not count — entity resolution never reads one, so a type
    // returned only by a mutation still needs its own declaration.
    let queried_types: HashSet<String> =
        schema.queries.iter().map(|q| q.return_type.clone()).collect();
    for typ in &mut schema.types {
        if queried_types.contains(&typ.name) {
            continue;
        }
        let Some(fields) = index.fields_for_type(&typ.name) else {
            continue;
        };
        for field_name in fields {
            let inject_source = format!("jwt:{tenant_claim}");
            if typ.inject_params.is_empty() {
                typ.inject_params.insert(field_name.clone(), inject_source);
            } else if !typ.inject_params.contains_key(field_name) {
                bail!(
                    "Type '{}' is @tenant_id-annotated on field '{}' and no query returns \
                     it, so its scoping has to live on the type — but its explicit \
                     inject_params lack '{}'. Add `inject_params.{} = \"{}\"`, or remove \
                     the explicit inject_params to use auto-injection.",
                    typ.name,
                    field_name,
                    field_name,
                    field_name,
                    inject_source,
                );
            }
        }
    }

    refuse_undeclared_root_types(schema, &index, tenant_claim, shared_types)
}

/// Refuse a type a root returns that carries no `@tenant_id` and is not declared shared, and a
/// type declared shared that carries one.
fn refuse_undeclared_root_types(
    schema: &IntermediateSchema,
    index: &AnnotatedTypeIndex,
    tenant_claim: &str,
    shared_types: &[String],
) -> Result<()> {
    let is_shared = |name: &str| shared_types.iter().any(|shared| shared == name);
    let mut contradictions: Vec<String> = schema
        .types
        .iter()
        .filter(|typ| is_shared(&typ.name) && index.fields_for_type(&typ.name).is_some())
        .map(|typ| format!("'{}'", typ.name))
        .collect();
    if !contradictions.is_empty() {
        contradictions.sort();
        bail!(
            "type(s) {} are listed in [tenancy] shared_types and carry a @tenant_id \
             field: a type cannot be both per-tenant and shared. Remove the type from \
             shared_types, or the @tenant_id directive from its field.",
            contradictions.join(", ")
        );
    }

    let tenant_source = format!("jwt:{tenant_claim}");
    let mut roots: Vec<(String, &str)> = Vec::new();
    for query in &schema.queries {
        roots.push((format!("query '{}'", query.name), &query.return_type));
    }
    for mutation in &schema.mutations {
        if !mutation.inject.values().any(|source| *source == tenant_source) {
            roots.push((format!("mutation '{}'", mutation.name), &mutation.return_type));
        }
    }
    for subscription in &schema.subscriptions {
        roots.push((format!("subscription '{}'", subscription.name), &subscription.return_type));
    }
    let queried: HashSet<&str> = schema.queries.iter().map(|q| q.return_type.as_str()).collect();
    for entity in federation_entities(schema) {
        if !queried.contains(entity) {
            roots.push(("federation `_entities`".to_string(), entity));
        }
    }

    let mut undeclared: Vec<String> = Vec::new();
    for (root, return_type) in roots {
        for type_name in object_types_returned(schema, return_type) {
            if index.fields_for_type(type_name).is_none() && !is_shared(type_name) {
                let entry = format!("'{type_name}' (returned by {root})");
                if !undeclared.contains(&entry) {
                    undeclared.push(entry);
                }
            }
        }
    }
    if undeclared.is_empty() {
        return Ok(());
    }
    bail!(
        "[tenancy] mode = \"row\" scopes a root by the @tenant_id field of the type it \
         returns, and these types have none and are not declared shared:\n  - {}\n\
         Mark the field that holds the tenant on each per-tenant type with @tenant_id. If every \
         tenant reads the same rows of a type, list it in [tenancy] shared_types.",
        undeclared.join("\n  - ")
    )
}

/// The object types a root returning `return_type` serves: the type itself, a union's
/// members, or an interface's implementors. Scalars and enums carry no rows to scope.
fn object_types_returned<'a>(schema: &'a IntermediateSchema, return_type: &'a str) -> Vec<&'a str> {
    if let Some(union) = schema.unions.iter().find(|u| u.name == return_type) {
        return union.member_types.iter().map(String::as_str).collect();
    }
    if schema.interfaces.iter().any(|i| i.name == return_type) {
        return schema
            .types
            .iter()
            .filter(|typ| typ.implements.iter().any(|i| i == return_type))
            .map(|typ| typ.name.as_str())
            .collect();
    }
    schema
        .types
        .iter()
        .filter(|typ| typ.name == return_type)
        .map(|typ| typ.name.as_str())
        .collect()
}

/// The federation entities `_entities` resolves: none unless federation is enabled.
fn federation_entities(schema: &IntermediateSchema) -> Vec<&str> {
    let Some(federation) = schema.federation_config.as_ref() else {
        return Vec::new();
    };
    if federation.get("enabled").and_then(serde_json::Value::as_bool) != Some(true) {
        return Vec::new();
    }
    federation.get("entities").and_then(serde_json::Value::as_array).map_or_else(
        Vec::new,
        |entities| {
            entities
                .iter()
                .filter_map(|entity| entity.get("name").and_then(serde_json::Value::as_str))
                .collect()
        },
    )
}
