//! Auto-synthesis of the typed graphql-cascade surface (per-mutation `cascade = true`).
//!
//! The graphql-cascade spec models a mutation's affected-entity set as a typed
//! `cascade` field on the mutation *payload* — not on the entity — so normalized
//! caches never store a `cascade` key against an entity. This pass realizes that
//! shape in the compiled schema, following the spec's own types
//! (`graphql-cascade/reference/cascade_base.graphql`).
//!
//! When ≥1 mutation has `cascade = true`, it synthesizes, once:
//! - a `CascadeNode` interface (`id: ID!`), auto-implemented on every queryable
//!   entity type (view-backed, non-error) so any entity can ride in a cascade and
//!   be selected via an inline fragment. Deliberately distinct from the relay
//!   `Node` interface (which carries global-id refetch semantics);
//! - the `CascadeOperation` enum (`CREATED`/`UPDATED`/`DELETED`);
//! - `UpdatedEntity` (`id`, `operation`, `entity: CascadeNode!`) for created/updated
//!   rows, and `DeletedEntity` (`id`, `deletedAt`) for deleted rows — a deleted row
//!   has no entity body to project, so it is a distinct type (a shared type with a
//!   non-null `entity` would be unsatisfiable);
//! - the `CascadeUpdates` envelope (`updated: [UpdatedEntity!]!`,
//!   `deleted: [DeletedEntity!]!`).
//!
//! Then, per cascade mutation, it synthesizes a `<Mutation>Payload`
//! (`entity: <ReturnType>`, `cascade: CascadeUpdates`, `updatedFields: [String!]!`)
//! and rewrites the mutation's return type to that payload. Running before
//! [`super::mutation_error_union`] makes the payload the success member of the
//! result union (`<Mutation>Result = <Mutation>Payload | MutationError`).
//!
//! Scope: `CascadeUpdates.metadata` (timestamp / depth / affectedCount /
//! truncation) is synthesized here; `.invalidations` is added in Phase 05 — until
//! then this is a spec-*aligned* subset, not the full envelope.
//!
//! It is idempotent (names already owned by a real type/interface/enum are left
//! alone) and inert unless a mutation opts in — so a schema with no cascade
//! mutations is byte-identical to one compiled before this pass existed.

use std::collections::HashSet;

use fraiseql_core::schema::{
    CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldDenyPolicy,
    FieldType, InterfaceDefinition, TypeDefinition,
};
use tracing::warn;

/// The auto-implemented cascade interface. Distinct from the relay `Node`
/// interface (which carries global-id refetch semantics).
const CASCADE_NODE: &str = "CascadeNode";
/// The cascade operation enum (`CREATED`/`UPDATED`/`DELETED`).
const CASCADE_OPERATION: &str = "CascadeOperation";
/// A created/updated cascade entry (`id`, `operation`, typed `entity`).
const UPDATED_ENTITY: &str = "UpdatedEntity";
/// A deleted cascade entry (`id`, `deletedAt`) — carries no entity body.
const DELETED_ENTITY: &str = "DeletedEntity";
/// The cascade envelope carried on a mutation payload.
const CASCADE_UPDATES: &str = "CascadeUpdates";
/// Metadata about a cascade (timestamp / depth / counts / truncation).
const CASCADE_METADATA: &str = "CascadeMetadata";

/// Synthesize the cascade interface, enum, envelope types, and per-mutation
/// payload wrappers, then rewrite cascade mutations to return their payloads.
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
    let existing_enum_names: HashSet<String> =
        schema.enums.iter().map(|e| e.name.clone()).collect();

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

    // 2. Shared enum + envelope types.
    if !existing_enum_names.contains(CASCADE_OPERATION) {
        schema.enums.push(cascade_operation_enum());
    }
    if !existing_type_names.contains(UPDATED_ENTITY) {
        schema.types.push(updated_entity_type());
    }
    if !existing_type_names.contains(DELETED_ENTITY) {
        schema.types.push(deleted_entity_type());
    }
    if !existing_type_names.contains(CASCADE_METADATA) {
        schema.types.push(cascade_metadata_type());
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

/// Build a synthetic output field. Synthetic cascade fields carry no
/// scope/authz/encryption of their own — enforcement happens per cascade entity
/// at runtime (Phase 03).
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

/// The `CascadeOperation` enum (`CREATED`/`UPDATED`/`DELETED`).
fn cascade_operation_enum() -> EnumDefinition {
    EnumDefinition::new(CASCADE_OPERATION)
        .with_value(EnumValueDefinition::new("CREATED").with_description("The entity was created."))
        .with_value(EnumValueDefinition::new("UPDATED").with_description("The entity was updated."))
        .with_value(EnumValueDefinition::new("DELETED").with_description("The entity was deleted."))
        .with_description("The kind of change a cascade entry records.")
}

/// The `UpdatedEntity` type (`id`, `operation`, `entity: CascadeNode!`) for
/// created/updated rows.
fn updated_entity_type() -> TypeDefinition {
    synth_type(
        UPDATED_ENTITY,
        vec![
            synth_field("id", FieldType::Id, false, "The affected entity's global ID."),
            synth_field(
                "operation",
                FieldType::Enum(CASCADE_OPERATION.to_string()),
                false,
                "Whether the entity was created or updated.",
            ),
            synth_field(
                "entity",
                FieldType::Interface(CASCADE_NODE.to_string()),
                false,
                "The affected entity, projected and field-authorized like a queried entity.",
            ),
        ],
        "An entity created or updated by a mutation's cascade.",
    )
}

/// The `DeletedEntity` type (`id`, `deletedAt`). Carries no `entity` body — the
/// row is gone, so there is nothing to project (a non-null `entity` here would be
/// unsatisfiable).
fn deleted_entity_type() -> TypeDefinition {
    synth_type(
        DELETED_ENTITY,
        vec![
            synth_field("id", FieldType::Id, false, "The deleted entity's global ID."),
            synth_field(
                "deletedAt",
                FieldType::DateTime,
                false,
                "When the entity was deleted.",
            ),
        ],
        "An entity deleted by a mutation's cascade.",
    )
}

/// The `CascadeUpdates` envelope carried on a cascade mutation's payload.
///
/// `invalidations` is added in Phase 05 — until then this is a spec-aligned subset.
fn cascade_updates_type() -> TypeDefinition {
    synth_type(
        CASCADE_UPDATES,
        vec![
            synth_field(
                "updated",
                FieldType::List(Box::new(FieldType::Object(UPDATED_ENTITY.to_string()))),
                false,
                "Entities created or updated by the mutation.",
            ),
            synth_field(
                "deleted",
                FieldType::List(Box::new(FieldType::Object(DELETED_ENTITY.to_string()))),
                false,
                "Entities deleted by the mutation.",
            ),
            synth_field(
                "metadata",
                FieldType::Object(CASCADE_METADATA.to_string()),
                false,
                "Timestamp, depth, affected count, and truncation status of the cascade.",
            ),
        ],
        "The set of entities affected by a mutation, per the graphql-cascade spec.",
    )
}

/// The `CascadeMetadata` type (graphql-cascade `metadata.schema.json`, plus the
/// `truncated` / `originalCount` fields the spec's size-limit section adds when a
/// cascade is truncated).
fn cascade_metadata_type() -> TypeDefinition {
    synth_type(
        CASCADE_METADATA,
        vec![
            synth_field(
                "timestamp",
                FieldType::DateTime,
                false,
                "Server timestamp when the mutation executed (ISO 8601).",
            ),
            synth_field(
                "transactionId",
                FieldType::Id,
                true,
                "Transaction ID for tracking (optional).",
            ),
            synth_field("depth", FieldType::Int, false, "Maximum relationship depth traversed."),
            synth_field(
                "affectedCount",
                FieldType::Int,
                false,
                "Number of entities in this (possibly truncated) cascade.",
            ),
            synth_field(
                "truncated",
                FieldType::Boolean,
                false,
                "Whether the cascade was truncated to satisfy the affected-entity limit.",
            ),
            synth_field(
                "originalCount",
                FieldType::Int,
                true,
                "The pre-truncation affected count, present only when truncated.",
            ),
        ],
        "Metadata about a mutation's cascade.",
    )
}

/// The per-mutation `<Name>Payload` wrapper (`entity`, `cascade`, `updatedFields`).
///
/// `updatedFields` (the #433 selection-gated surface) rehomes here from the entity,
/// which is now nested under `entity:`.
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
            synth_field(
                "updatedFields",
                FieldType::List(Box::new(FieldType::String)),
                false,
                "GraphQL field names on the primary entity changed by this mutation (#433).",
            ),
        ],
        &format!("Payload of a cascade mutation returning {entity_type}: the entity plus its cascade."),
    )
}
