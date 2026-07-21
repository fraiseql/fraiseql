//! Auto-synthesis of the typed graphql-cascade surface (per-mutation `cascade = true`).
//!
//! The graphql-cascade spec models a mutation's affected-entity set as a typed
//! `cascade` field on the mutation *payload* — not on the entity — so normalized
//! caches never store a `cascade` key against an entity. This pass realizes that
//! shape in the compiled schema, following the spec's own types
//! (`graphql-cascade/reference/cascade_base.graphql`).
//!
//! When ≥1 mutation has `cascade = true`, it synthesizes, once:
//! - a `CascadeNode` interface (`id: ID!`), auto-implemented on every queryable entity type
//!   (view-backed, non-error) so any entity can ride in a cascade and be selected via an inline
//!   fragment. Deliberately distinct from the relay `Node` interface (which carries global-id
//!   refetch semantics);
//! - the `CascadeOperation` enum (`CREATED`/`UPDATED`/`DELETED`);
//! - `UpdatedEntity` (`id`, `operation`, `entity: CascadeNode!`) for created/updated rows, and
//!   `DeletedEntity` (`id`, `deletedAt`) for deleted rows — a deleted row has no entity body to
//!   project, so it is a distinct type (a shared type with a non-null `entity` would be
//!   unsatisfiable);
//! - the `CascadeUpdates` envelope (`updated: [UpdatedEntity!]!`, `deleted: [DeletedEntity!]!`).
//!
//! Then, per cascade mutation, it synthesizes a `<Mutation>Payload`
//! (`entity: <ReturnType>`, `cascade: CascadeUpdates`, `updatedFields: [String!]!`)
//! and rewrites the mutation's return type to that payload. Running before
//! [`super::mutation_error_union`] makes the payload the success member of the
//! result union (`<Mutation>Result = <Mutation>Payload | MutationError`).
//!
//! The full spec envelope is synthesized: `updated` / `deleted` / `metadata` /
//! `invalidations` (the latter with the `QueryInvalidation` type and the
//! `InvalidationStrategy` / `InvalidationScope` enums).
//!
//! It is idempotent (names already owned by a real type/interface/enum are left
//! alone) and inert unless a mutation opts in — so a schema with no cascade
//! mutations is byte-identical to one compiled before this pass existed.

use std::collections::{HashMap, HashSet, VecDeque};

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
/// A client-side cache-invalidation hint.
const QUERY_INVALIDATION: &str = "QueryInvalidation";
/// How a client should handle an invalidation hint.
const INVALIDATION_STRATEGY: &str = "InvalidationStrategy";
/// The scope of an invalidation hint.
const INVALIDATION_SCOPE: &str = "InvalidationScope";

/// Synthesize the cascade interface, enum, envelope types, and per-mutation
/// payload wrappers, then rewrite cascade mutations to return their payloads.
///
/// Inert unless a mutation has `cascade = true`.
pub(super) fn synthesize_cascade_types(schema: &mut CompiledSchema) -> anyhow::Result<()> {
    let cascade_mutation_indices: Vec<usize> = schema
        .mutations
        .iter()
        .enumerate()
        .filter(|(_, m)| m.cascade)
        .map(|(idx, _)| idx)
        .collect();
    if cascade_mutation_indices.is_empty() {
        return Ok(());
    }

    // Enforce the graphql-cascade Node contract *before* forcing `implements
    // CascadeNode`: an entity that cannot back `id: ID!` would otherwise yield a
    // compiled schema that `validate()` rejects with a swallowed "missing field
    // 'id'". Fail fast with one aggregated, actionable error instead.
    // Reference path a cascade mutation reaches each type through (#653 proposal 4),
    // computed up front so its immutable borrow ends before the annotate closure below
    // captures the owned map.
    let reference_paths = build_reference_paths(schema);

    // A declared value object that also carries a source is a contradiction the SDK cannot
    // emit (it suppresses the source for an `embedded` type) — so it is hand-authored, and
    // most likely a mistake. The declaration wins (the type is exempt either way); say so
    // rather than resolving it silently.
    for ty in schema.types.iter().filter(|t| t.embedded && !t.sql_source.as_str().is_empty()) {
        warn!(
            type_name = %ty.name,
            sql_source = %ty.sql_source.as_str(),
            "cascade: type declares embedded=True but also declares a sql_source; treating it as \
             an embedded value object (exempt from the CascadeNode id contract) and ignoring the \
             source"
        );
    }

    super::interface_conformance::enforce_node_id_conformance(
        schema.types.iter().filter(|t| is_queryable_entity(t)),
        "cascade requires `id: ID!` on every cascade entity (the graphql-cascade CascadeNode \
         interface requires it)",
        "remove `cascade` from the mutations that return them",
        |ty| annotate_cascade_offender(ty, &reference_paths),
    )?;

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
        if is_queryable_entity(ty) && !ty.implements.iter().any(|i| i == CASCADE_NODE) {
            ty.implements.push(CASCADE_NODE.to_string());
        }
    }

    // 2. Shared enums + envelope types.
    if !existing_enum_names.contains(CASCADE_OPERATION) {
        schema.enums.push(cascade_operation_enum());
    }
    if !existing_enum_names.contains(INVALIDATION_STRATEGY) {
        schema.enums.push(invalidation_strategy_enum());
    }
    if !existing_enum_names.contains(INVALIDATION_SCOPE) {
        schema.enums.push(invalidation_scope_enum());
    }
    // Track exactly which envelope value types this pass synthesized (a name already owned by
    // a real user type is left alone), so federation can mark precisely those `@shareable`.
    let mut synthesized_envelopes: Vec<&'static str> = Vec::new();
    for (name, build) in [
        (UPDATED_ENTITY, updated_entity_type as fn() -> TypeDefinition),
        (DELETED_ENTITY, deleted_entity_type),
        (CASCADE_METADATA, cascade_metadata_type),
        (QUERY_INVALIDATION, query_invalidation_type),
        (CASCADE_UPDATES, cascade_updates_type),
    ] {
        if !existing_type_names.contains(name) {
            schema.types.push(build());
            synthesized_envelopes.push(name);
        }
    }

    // Federation (#698): the synthesized envelope value types are structurally identical in
    // every cascade-enabled subgraph and carry no independent identity, so a supergraph that
    // composes two such subgraphs rejects them with `INVALID_FIELD_SHARING` unless they are
    // `@shareable` in each. Mark exactly the ones this pass synthesized — the same treatment the
    // authored `MutationError` value type already gets via `shareable_types`. No-op without a
    // federation block; the per-mutation `<Name>Payload` types are uniquely named per entity and
    // never collide, so they are deliberately not marked.
    if let Some(fed) = schema.federation.as_mut() {
        for name in synthesized_envelopes {
            if !fed.shareable_types.iter().any(|t| t == name) {
                fed.shareable_types.push(name.to_string());
            }
        }
    }

    // 3. Per-mutation payload wrapper + return-type rewrite. Plan up front so the mutation list
    //    isn't borrowed while pushing synthesized types.
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

    Ok(())
}

/// A queryable entity type: the `CascadeNode` interface is auto-implemented on exactly
/// these, and `id: ID!` is enforced on exactly these. Three exclusion legs:
///
/// - **error types** (`is_error`) — populated from `mutation_response.metadata`, never a cache
///   node;
/// - **framework-internal projections** (`internal`, #665) — the change-log / checkpoint
///   bookkeeping views `inject_changelog` synthesizes are the change-capture *mechanism*, not
///   cascade-deliverable entities; `TransportCheckpoint` in particular has no `id` by design and
///   could never back the CascadeNode contract;
/// - **empty-source synthetics** — the cascade envelope/payload types and `MutationError` carry an
///   empty `sql_source`, as do relay connection/edge wrappers;
/// - **declared value objects** (`embedded`, #687) — the author stated the type has no independent
///   identity, so it is delivered inside its parent's payload rather than as a cache node of its
///   own. The SDK emits no source for such a type (which the empty-source leg would already
///   exempt); this leg is the *declared*, source-independent guard, so a hand-authored `embedded`
///   type that also carries a source still resolves as embedded.
///
/// This one predicate feeds BOTH the `id: ID!` enforcement and the
/// `implements CascadeNode` auto-loop, so the two can never key off divergent sets.
fn is_queryable_entity(ty: &TypeDefinition) -> bool {
    !ty.is_error && !ty.internal && !ty.sql_source.as_str().is_empty() && !ty.embedded
}

/// Explain (#653) why a type that failed the `id: ID!` contract was classified a cascade
/// entity, and how a cascade mutation reaches it. Returns `None` when there is nothing to
/// add. The signal (its declared `sql_source`) is only surfaced for a type with **no**
/// `id` — a missing-id type is most likely an embedded value object the SDK gave a
/// synthesized source; a type with a wrong-typed `id` is a genuine entity, where the
/// source is not the issue.
///
/// The suggested fix names both exits (#687): declare the type `embedded=True` if it is a
/// value object, or give it an `id: ID!` if it is an entity. It deliberately does *not*
/// tell the author to "declare no source" — under the SDK every `@fraiseql.type` gets a
/// synthesized source, so that was a state they had no way to reach; declaring `embedded`
/// is what suppresses it.
fn annotate_cascade_offender(
    ty: &TypeDefinition,
    reference_paths: &HashMap<String, String>,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let has_id = ty.fields.iter().any(|f| f.name == "id");
    if !has_id && !ty.sql_source.as_str().is_empty() {
        parts.push(format!(
            "classified as a cascade entity because it declares sql_source = \"{}\"; if it is an \
             embedded value object, mark it embedded=True (it will declare no source and be \
             exempt); if it is an entity, add id: ID!",
            ty.sql_source.as_str()
        ));
    }
    if let Some(path) = reference_paths.get(ty.name.as_str()) {
        parts.push(format!("reached via {path}"));
    }
    (!parts.is_empty()).then(|| parts.join("\n      "))
}

/// For each cascade mutation, walk its return type's object graph (breadth-first) and
/// record the first — shortest — path that reaches each type, e.g.
/// `createOrder → Order.total → Money` (#653 proposal 4). A type not reachable from any
/// cascade mutation gets no entry (it is a top-level entity, legitimately identity-bearing).
fn build_reference_paths(schema: &CompiledSchema) -> HashMap<String, String> {
    let type_by_name: HashMap<&str, &TypeDefinition> =
        schema.types.iter().map(|t| (t.name.as_str(), t)).collect();
    let mut paths: HashMap<String, String> = HashMap::new();
    for m in schema.mutations.iter().filter(|m| m.cascade) {
        let root = m.return_type.as_str();
        let mut seen: HashSet<String> = HashSet::from([root.to_string()]);
        let mut queue: VecDeque<(String, String)> =
            VecDeque::from([(root.to_string(), format!("{} → {root}", m.name))]);
        while let Some((tname, path)) = queue.pop_front() {
            paths.entry(tname.clone()).or_insert(path.clone());
            let Some(ty) = type_by_name.get(tname.as_str()) else {
                continue;
            };
            for f in &ty.fields {
                if let Some(child) = object_type_name(&f.field_type) {
                    if seen.insert(child.to_string()) {
                        queue
                            .push_back((child.to_string(), format!("{path}.{} → {child}", f.name)));
                    }
                }
            }
        }
    }
    paths
}

/// The named object type a field ultimately refers to, unwrapping `List` wrappers.
/// Scalars, enums, interfaces, and JSON return `None`.
fn object_type_name(field_type: &FieldType) -> Option<&str> {
    match field_type {
        FieldType::Object(name) => Some(name.as_str()),
        FieldType::List(inner) => object_type_name(inner),
        _ => None,
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
/// at runtime.
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
        name: name.into(),
        sql_source: String::new().into(),
        jsonb_column: String::new(),
        fields,
        description: Some(desc.to_string()),
        sql_projection_hint: None,
        implements: Vec::new(),
        requires_role: None,
        is_error: false,
        relay: false,
        internal: false,
        embedded: false,
        relationships: Vec::new(),
        subscription_policy: None,
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
            synth_field("deletedAt", FieldType::DateTime, false, "When the entity was deleted."),
        ],
        "An entity deleted by a mutation's cascade.",
    )
}

/// The `CascadeUpdates` envelope carried on a cascade mutation's payload
/// (`updated` / `deleted` / `metadata` / `invalidations`).
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
            synth_field(
                "invalidations",
                FieldType::List(Box::new(FieldType::Object(QUERY_INVALIDATION.to_string()))),
                false,
                "Client-side cache-invalidation hints emitted by the mutation.",
            ),
        ],
        "The set of entities affected by a mutation, per the graphql-cascade spec.",
    )
}

/// The `InvalidationStrategy` enum (how a client handles an invalidation hint).
fn invalidation_strategy_enum() -> EnumDefinition {
    EnumDefinition::new(INVALIDATION_STRATEGY)
        .with_value(
            EnumValueDefinition::new("INVALIDATE")
                .with_description("Mark the query stale; refetch on next access."),
        )
        .with_value(
            EnumValueDefinition::new("REFETCH").with_description("Immediately refetch the query."),
        )
        .with_value(
            EnumValueDefinition::new("REMOVE").with_description("Remove the query from cache."),
        )
        .with_description("How a client should handle a cache-invalidation hint.")
}

/// The `InvalidationScope` enum (which queries an invalidation hint targets).
fn invalidation_scope_enum() -> EnumDefinition {
    EnumDefinition::new(INVALIDATION_SCOPE)
        .with_value(
            EnumValueDefinition::new("EXACT")
                .with_description("Only the exact query with exact arguments."),
        )
        .with_value(
            EnumValueDefinition::new("PREFIX")
                .with_description("All queries whose name matches the prefix."),
        )
        .with_value(
            EnumValueDefinition::new("PATTERN")
                .with_description("All queries matching the glob-style pattern."),
        )
        .with_value(EnumValueDefinition::new("ALL").with_description("All queries."))
        .with_description("The scope a cache-invalidation hint targets.")
}

/// The `QueryInvalidation` type (a client-side cache-invalidation hint).
fn query_invalidation_type() -> TypeDefinition {
    synth_type(
        QUERY_INVALIDATION,
        vec![
            synth_field("queryName", FieldType::String, true, "Query name to invalidate."),
            synth_field("queryHash", FieldType::String, true, "Hash of the query for exact match."),
            synth_field(
                "arguments",
                FieldType::Json,
                true,
                "Arguments identifying the query to invalidate.",
            ),
            synth_field(
                "queryPattern",
                FieldType::String,
                true,
                "Glob-style pattern matching queries to invalidate.",
            ),
            synth_field(
                "strategy",
                FieldType::Enum(INVALIDATION_STRATEGY.to_string()),
                false,
                "How the client should handle the invalidation.",
            ),
            synth_field(
                "scope",
                FieldType::Enum(INVALIDATION_SCOPE.to_string()),
                false,
                "The scope of the invalidation.",
            ),
        ],
        "A client-side cache-invalidation hint emitted by a mutation.",
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
        &format!(
            "Payload of a cascade mutation returning {entity_type}: the entity plus its cascade."
        ),
    )
}
