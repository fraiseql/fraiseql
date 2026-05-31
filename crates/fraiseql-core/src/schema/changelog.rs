//! Changelog GraphQL-exposure schema injection (issue #149).
//!
//! When [`CompiledSchema::changelog`](super::CompiledSchema) is present with
//! `expose = true`, [`inject_changelog`] adds two read-only object types
//! (`EntityChangeLog`, `TransportCheckpoint`), a cursor-paginated list query
//! (`entity_change_logs`), a point-lookup query (`transport_checkpoint`), and a
//! checkpoint upsert mutation (`upsert_transport_checkpoint`).
//!
//! The generated objects are indistinguishable from user-authored ones: the types
//! are view-backed (`{schema}.v_entity_change_log` / `{schema}.v_transport_checkpoint`),
//! the list query relies on the standard `auto_params` filter/sort/limit machinery,
//! and access is gated by `requires_role` exactly like any other operation. Cursor
//! pagination is the standard generic-filter pattern:
//!
//! ```graphql
//! entity_change_logs(
//!   where:   { pk_entity_change_log: { gt: $cursor }, object_type: { eq: "User" } }
//!   orderBy: [{ field: "pk_entity_change_log", direction: ASC }]
//!   limit:   100
//! ) { pk_entity_change_log object_type object_data created_at }
//! ```

use super::{
    AutoParams, ChangelogConfig, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
    QueryDefinition, TypeDefinition, compiled::ArgumentDefinition,
};

/// GraphQL type name for change-log entries.
const ENTITY_CHANGE_LOG: &str = "EntityChangeLog";
/// GraphQL type name for transport checkpoints.
const TRANSPORT_CHECKPOINT: &str = "TransportCheckpoint";

/// Inject the changelog GraphQL surface into `schema` when `[changelog] expose = true`.
///
/// No-op when `schema.changelog` is absent or `expose = false`. Rebuilds the schema's
/// lookup indexes so injected operations are immediately resolvable in-memory (the
/// serialization round-trip rebuilds them too, so this is also safe for the compiler).
pub fn inject_changelog(schema: &mut CompiledSchema) {
    let Some(cfg) = schema.changelog.clone() else {
        return;
    };
    if !cfg.expose {
        return;
    }

    schema.types.push(entity_change_log_type(&cfg));
    schema.types.push(transport_checkpoint_type(&cfg));
    schema.queries.push(entity_change_logs_query(&cfg));
    schema.queries.push(transport_checkpoint_query(&cfg));
    schema.mutations.push(upsert_checkpoint_mutation(&cfg));

    schema.build_indexes();
}

/// `EntityChangeLog` — read-only projection over `{schema}.v_entity_change_log`.
fn entity_change_log_type(cfg: &ChangelogConfig) -> TypeDefinition {
    let mut t =
        TypeDefinition::new(ENTITY_CHANGE_LOG, format!("{}.v_entity_change_log", cfg.schema))
            .with_description(
                "An observer entity-change-log entry. Read-only; populated by the observer \
             system. Paginate via the `pk_entity_change_log` cursor.",
            )
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("pk_entity_change_log", FieldType::Int))
            .with_field(FieldDefinition::nullable("fk_customer_org", FieldType::String))
            .with_field(FieldDefinition::nullable("fk_contact", FieldType::String))
            .with_field(FieldDefinition::new("object_type", FieldType::String))
            .with_field(FieldDefinition::new("object_id", FieldType::String))
            .with_field(FieldDefinition::new("modification_type", FieldType::String))
            .with_field(FieldDefinition::nullable("change_status", FieldType::String))
            .with_field(FieldDefinition::new("object_data", FieldType::Json))
            .with_field(FieldDefinition::nullable("extra_metadata", FieldType::Json))
            .with_field(FieldDefinition::new("created_at", FieldType::DateTime));
    t.requires_role.clone_from(&cfg.read_role);
    t
}

/// `TransportCheckpoint` — read-only projection over `{schema}.v_transport_checkpoint`.
fn transport_checkpoint_type(cfg: &ChangelogConfig) -> TypeDefinition {
    let mut t =
        TypeDefinition::new(TRANSPORT_CHECKPOINT, format!("{}.v_transport_checkpoint", cfg.schema))
            .with_description(
                "A consumer's change-log cursor checkpoint, keyed by `transport_name`.",
            )
            .with_field(FieldDefinition::new("transport_name", FieldType::String))
            .with_field(FieldDefinition::new("last_pk", FieldType::Int))
            .with_field(FieldDefinition::new("updated_at", FieldType::DateTime));
    t.requires_role.clone_from(&cfg.read_role);
    t
}

/// `entity_change_logs` — cursor-paginated list query using the generic filter machinery.
fn entity_change_logs_query(cfg: &ChangelogConfig) -> QueryDefinition {
    let mut q = QueryDefinition::new("entity_change_logs", ENTITY_CHANGE_LOG)
        .returning_list()
        .with_sql_source(format!("{}.v_entity_change_log", cfg.schema));
    q.description = Some(format!(
        "Cursor-paginate the observer change-log. Poll with \
         `where: {{ pk_entity_change_log: {{ gt: $cursor }} }}`, \
         `orderBy: [{{ field: \"pk_entity_change_log\", direction: ASC }}]`, and a \
         `limit` (server max {}). Optional equality filters: object_type, modification_type.",
        cfg.max_limit
    ));
    q.auto_params = AutoParams {
        has_where:    true,
        has_order_by: true,
        has_limit:    true,
        has_offset:   false,
    };
    // The change-log is append-only and polled in real time — never serve cached pages.
    q.cache_ttl_seconds = Some(0);
    q.requires_role.clone_from(&cfg.read_role);
    q
}

/// `transport_checkpoint(transport_name)` — point lookup of a single checkpoint.
fn transport_checkpoint_query(cfg: &ChangelogConfig) -> QueryDefinition {
    let mut q = QueryDefinition::new("transport_checkpoint", TRANSPORT_CHECKPOINT)
        .with_sql_source(format!("{}.v_transport_checkpoint", cfg.schema));
    q.nullable = true;
    q.description = Some("Fetch one consumer's checkpoint by transport_name.".to_string());
    // Named scalar arg → `WHERE data->>'transport_name' = $1` equality lookup.
    q.arguments = vec![ArgumentDefinition::new("transport_name", FieldType::String)];
    q.cache_ttl_seconds = Some(0);
    q.requires_role.clone_from(&cfg.read_role);
    q
}

/// `upsert_transport_checkpoint(transport_name, last_pk)` — advance a consumer cursor.
fn upsert_checkpoint_mutation(cfg: &ChangelogConfig) -> MutationDefinition {
    let mut m = MutationDefinition::new("upsert_transport_checkpoint", TRANSPORT_CHECKPOINT);
    m.sql_source = Some(format!("{}.fn_upsert_transport_checkpoint", cfg.schema));
    m.description =
        Some("Create or advance a consumer's change-log checkpoint (idempotent).".to_string());
    m.arguments = vec![
        ArgumentDefinition::new("transport_name", FieldType::String),
        ArgumentDefinition::new("last_pk", FieldType::Int),
    ];
    m.requires_role.clone_from(&cfg.write_role);
    m
}

#[cfg(test)]
mod tests;
