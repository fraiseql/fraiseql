//! Auto-synthesis of mutation result unions (`[fraiseql.mutations] auto_error_union`).
//!
//! Every mutation backed by the `app.mutation_response` composite can resolve to a
//! success entity *or* an error, and the runtime already discriminates on
//! `succeeded`/`error_class`. What it needs is a GraphQL union to resolve against.
//! Declaring `Entity | MutationError` by hand for every mutation is pure, uniform
//! boilerplate — and forgetting it silently maps failures onto the success type.
//!
//! When enabled, this pass synthesizes a single shared `MutationError` object type
//! and, for each object-returning mutation, a `<Mutation>Result` union of the
//! success entity and `MutationError`, then rewrites the mutation's return type to
//! that union. It is opt-in, idempotent (a mutation already returning a union is
//! left alone), and never clobbers a name a real type already owns.

use std::collections::HashSet;

use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, TypeDefinition, UnionDefinition,
};
use tracing::warn;

/// The shared error type's name. Reused across every synthesized result union.
const ERROR_TYPE: &str = "MutationError";

/// Synthesize the shared `MutationError` type and per-mutation result unions, and
/// rewrite object-returning mutations to their unions.
///
/// Mutations whose return type is already a union, or is a scalar/enum/error/unknown
/// type, are left untouched (explicit declarations always win).
pub(super) fn synthesize_mutation_error_unions(schema: &mut CompiledSchema) {
    let existing_union_names: HashSet<String> =
        schema.unions.iter().map(|u| u.name.clone()).collect();
    let existing_type_names: HashSet<String> =
        schema.types.iter().map(|t| t.name.to_string()).collect();
    // Object (non-error) types are the only valid success members.
    let object_type_names: HashSet<String> = schema
        .types
        .iter()
        .filter(|t| !t.is_error)
        .map(|t| t.name.to_string())
        .collect();

    // Plan the rewrites up front so the mutation list isn't borrowed while we push
    // synthesized unions: (mutation index, success type, union name, mutation name).
    let mut plans: Vec<(usize, String, String, String)> = Vec::new();
    for (idx, mutation) in schema.mutations.iter().enumerate() {
        let return_type = mutation.return_type.clone();
        // Already a union → explicit declaration wins.
        if existing_union_names.contains(&return_type) {
            continue;
        }
        // Only wrap when the return type names a known, non-error object type.
        if !object_type_names.contains(&return_type) {
            continue;
        }
        let union_name = result_union_name(&mutation.name);
        plans.push((idx, return_type, union_name, mutation.name.clone()));
    }

    if plans.is_empty() {
        return;
    }

    // Synthesize the shared error type once.
    if !existing_type_names.contains(ERROR_TYPE) {
        schema.types.push(mutation_error_type());
    }

    let mut created_unions: HashSet<String> = HashSet::new();
    for (idx, success_type, union_name, mutation_name) in plans {
        // A non-union type already owns this name — don't clobber it; leave the
        // mutation's return type as the bare success entity.
        if existing_type_names.contains(&union_name) {
            warn!(
                mutation = %mutation_name,
                union = %union_name,
                "auto_error_union: a type already uses the result-union name; leaving this \
                 mutation's return type unchanged",
            );
            continue;
        }
        // Create the union unless one with this name already exists (user-declared
        // or produced earlier in this loop for a sibling mutation).
        if !existing_union_names.contains(&union_name) && !created_unions.contains(&union_name) {
            schema.unions.push(
                UnionDefinition::new(union_name.clone())
                    .with_members(vec![success_type, ERROR_TYPE.to_string()])
                    .with_description(format!(
                        "Result of the {mutation_name} mutation: the success entity or a \
                         MutationError."
                    )),
            );
            created_unions.insert(union_name.clone());
        }
        schema.mutations[idx].return_type = union_name;
    }
}

/// `createOrder` → `CreateOrderResult`. Uppercases the first character (mutation
/// names are camelCase by convention) and appends `Result`.
fn result_union_name(mutation_name: &str) -> String {
    let mut chars = mutation_name.chars();
    let pascal = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    format!("{pascal}Result")
}

/// Build the shared `MutationError` type. Its fields are populated by the runtime
/// from the `app.mutation_response` composite: `status`/`errorClass` from
/// `error_class`, `message` and `httpStatus` from the corresponding columns.
fn mutation_error_type() -> TypeDefinition {
    let field = |name: &str, field_type: FieldType, nullable: bool, desc: &str| FieldDefinition {
        name: name.into(),
        field_type,
        nullable,
        description: Some(desc.to_string()),
        default_value: None,
        vector_config: None,
        alias: None,
        deprecation: None,
        requires_scope: None,
        on_deny: FieldDenyPolicy::default(),
        authorize: false,
        encryption: None,
        hierarchy: None,
    };
    TypeDefinition {
        name:                ERROR_TYPE.into(),
        sql_source:          String::new().into(), // synthetic — never queried via SQL
        jsonb_column:        String::new(),
        fields:              vec![
            field(
                "status",
                FieldType::String,
                false,
                "Error class discriminator (e.g. \"not_found\", \"validation\").",
            ),
            field("message", FieldType::String, true, "Human-readable error message."),
            field("httpStatus", FieldType::Int, true, "Suggested HTTP status code."),
            field(
                "errorClass",
                FieldType::String,
                true,
                "Error classification (the same value surfaced as `status`).",
            ),
        ],
        description:         Some(
            "Shared mutation error type (auto-synthesized by auto_error_union). Populated from \
             the mutation_response composite."
                .to_string(),
        ),
        sql_projection_hint: None,
        implements:          Vec::new(),
        requires_role:       None,
        is_error:            true,
        relay:               false,
        relationships:       Vec::new(),
    }
}
