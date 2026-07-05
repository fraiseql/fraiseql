//! Auto-synthesis of the typed graphql-cascade surface (per-mutation `cascade = true`).
//!
//! The graphql-cascade spec models a mutation's affected-entity set as a typed
//! `cascade` field on the mutation *payload* — not on the entity — so normalized
//! caches never store a `cascade` key against an entity. This pass realizes that
//! shape in the compiled schema.
//!
//! When ≥1 mutation has `cascade = true`, it synthesizes, once:
//! - a `CascadeNode` interface (`id: ID!`), auto-implemented on every queryable
//!   entity type (view-backed, non-error) so any entity can ride in a cascade and
//!   be selected via an inline fragment;
//! - the `CascadeEntity` type (`id: ID!`, `entity: CascadeNode!`) and the
//!   `CascadeUpdates` envelope (`updated`/`deleted: [CascadeEntity!]!`,
//!   `truncated: Boolean!`).
//!
//! Then, per cascade mutation, it synthesizes a `<Mutation>Payload`
//! (`entity: <ReturnType>`, `cascade: CascadeUpdates`) and rewrites the mutation's
//! return type to that payload. Running before
//! [`super::mutation_error_union`] makes the payload the success member of the
//! result union (`<Mutation>Result = <Mutation>Payload | MutationError`).
//!
//! It is idempotent (names already owned by a real type/interface are left alone)
//! and inert unless a mutation opts in — so a schema with no cascade mutations is
//! byte-identical to one compiled before this pass existed.

use std::collections::HashSet;

use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, InterfaceDefinition,
    TypeDefinition,
};
use tracing::warn;

/// The auto-implemented cascade interface. Distinct from the relay `Node`
/// interface (which carries global-id refetch semantics).
const CASCADE_NODE: &str = "CascadeNode";
/// The per-entry cascade wrapper (`id` + typed `entity`).
const CASCADE_ENTITY: &str = "CascadeEntity";
/// The cascade envelope carried on a mutation payload.
const CASCADE_UPDATES: &str = "CascadeUpdates";

/// Synthesize the cascade interface, envelope types, and per-mutation payload
/// wrappers, then rewrite cascade mutations to return their payloads.
///
/// Inert unless a mutation has `cascade = true`.
pub(super) fn synthesize_cascade_types(schema: &mut CompiledSchema) {
    let cascade_mutation_indices: Vec<usize> = schema
        .mutations
        .iter()
        .enumerate()
        .filter(|(_, m)| m.cascade)
        .map(|(idx, _)| idx)
        .collect();
    if cascade_mutation_indices.is_empty() {
        return;
    }

    let existing_type_names: HashSet<String> =
        schema.types.iter().map(|t| t.name.to_string()).collect();
    let existing_interface_names: HashSet<String> =
        schema.interfaces.iter().map(|i| i.name.clone()).collect();

    // 1. CascadeNode interface, auto-implemented on every queryable entity type.
    if !existing_interface_names.contains(CASCADE_NODE) {
        schema.interfaces.push(cascade_node_interface());
    }
    for ty in &mut schema.types {
        // Queryable entity types only: view-backed and non-error. Synthetic types
        // (the cascade envelope, payloads, MutationError) have an empty sql_source
        // and are skipped, as are relay connection/edge wrappers.
        let is_queryable_entity = !ty.is_error && !ty.sql_source.as_str().is_empty();
        if is_queryable_entity && !ty.implements.iter().any(|i| i == CASCADE_NODE) {
            ty.implements.push(CASCADE_NODE.to_string());
        }
    }

    // 2. Shared envelope types (order: CascadeEntity before CascadeUpdates, which
    //    references it — purely cosmetic for SDL emission).
    if !existing_type_names.contains(CASCADE_ENTITY) {
        schema.types.push(cascade_entity_type());
    }
    if !existing_type_names.contains(CASCADE_UPDATES) {
        schema.types.push(cascade_updates_type());
    }

    // 3. Per-mutation payload wrapper + return-type rewrite. Plan up front so the
    //    mutation list isn't borrowed while pushing synthesized types.
    let plans: Vec<(usize, String, String)> = cascade_mutation_indices
        .iter()
        .map(|&idx| {
            let m = &schema.mutations[idx];
            (idx, m.return_type.clone(), payload_type_name(&m.name))
        })
        .collect();

    let mut created: HashSet<String> = HashSet::new();
    for (idx, entity_type, payload_name) in plans {
        // A real type already owns this name — don't clobber it; leave the
        // mutation's return type as the bare entity.
        if existing_type_names.contains(&payload_name) {
            warn!(
                mutation = %schema.mutations[idx].name,
                payload = %payload_name,
                "cascade: a type already uses the payload name; leaving this mutation's return \
                 type unchanged",
            );
            continue;
        }
        if !created.contains(&payload_name) {
            schema.types.push(cascade_payload_type(&payload_name, &entity_type));
            created.insert(payload_name.clone());
        }
        schema.mutations[idx].return_type = payload_name;
    }
}

/// `createUser` → `CreateUserPayload`. Uppercases the first character (mutation
/// names are camelCase by convention) and appends `Payload`.
fn payload_type_name(mutation_name: &str) -> String {
    let mut chars = mutation_name.chars();
    let pascal = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    format!("{pascal}Payload")
}

/// Build a synthetic, non-nullable output field. Synthetic cascade fields carry
/// no scope/authz/encryption of their own — enforcement happens per cascade
/// entity at runtime (Phase 03).
fn synth_field(name: &str, field_type: FieldType, nullable: bool, desc: &str) -> FieldDefinition {
    FieldDefinition {
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
    }
}

/// Build a synthetic object type (empty `sql_source` — never queried via SQL).
fn synth_type(name: &str, fields: Vec<FieldDefinition>, desc: &str) -> TypeDefinition {
    TypeDefinition {
        name:                name.into(),
        sql_source:          String::new().into(),
        jsonb_column:        String::new(),
        fields,
        description:         Some(desc.to_string()),
        sql_projection_hint: None,
        implements:          Vec::new(),
        requires_role:       None,
        is_error:            false,
        relay:               false,
        relationships:       Vec::new(),
    }
}

/// The `CascadeNode` interface (`id: ID!`).
fn cascade_node_interface() -> InterfaceDefinition {
    InterfaceDefinition::new(CASCADE_NODE)
        .with_field(synth_field("id", FieldType::Id, false, "The entity's global ID."))
        .with_description(
            "An entity that can ride in a mutation's cascade. Auto-implemented on every \
             queryable entity type; select concrete fields via an inline fragment.",
        )
}

/// The `CascadeEntity` type (`id: ID!`, `entity: CascadeNode!`).
fn cascade_entity_type() -> TypeDefinition {
    synth_type(
        CASCADE_ENTITY,
        vec![
            synth_field("id", FieldType::Id, false, "The affected entity's global ID."),
            synth_field(
                "entity",
                FieldType::Interface(CASCADE_NODE.to_string()),
                false,
                "The affected entity, projected and field-authorized like a queried entity.",
            ),
        ],
        "A single entity affected by a mutation's cascade.",
    )
}

/// The `CascadeUpdates` envelope carried on a cascade mutation's payload.
fn cascade_updates_type() -> TypeDefinition {
    let entity_list = || {
        FieldType::List(Box::new(FieldType::Object(CASCADE_ENTITY.to_string())))
    };
    synth_type(
        CASCADE_UPDATES,
        vec![
            synth_field("updated", entity_list(), false, "Entities created or updated by the mutation."),
            synth_field("deleted", entity_list(), false, "Entities deleted by the mutation."),
            synth_field(
                "truncated",
                FieldType::Boolean,
                false,
                "Whether the cascade was truncated to satisfy a response limit.",
            ),
        ],
        "The set of entities affected by a mutation, per the graphql-cascade spec.",
    )
}

/// The per-mutation `<Name>Payload` wrapper (`entity`, `cascade`).
fn cascade_payload_type(payload_name: &str, entity_type: &str) -> TypeDefinition {
    synth_type(
        payload_name,
        vec![
            synth_field(
                "entity",
                FieldType::Object(entity_type.to_string()),
                true,
                "The primary mutated entity.",
            ),
            synth_field(
                "cascade",
                FieldType::Object(CASCADE_UPDATES.to_string()),
                true,
                "All other entities affected by this mutation.",
            ),
        ],
        &format!("Payload of a cascade mutation returning {entity_type}: the entity plus its cascade."),
    )
}
