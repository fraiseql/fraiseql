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
    NamingConvention, QueryDefinition, TypeDefinition, compiled::ArgumentDefinition,
};
use crate::utils::casing::to_camel_case;

/// GraphQL type name for change-log entries.
const ENTITY_CHANGE_LOG: &str = "EntityChangeLog";
/// GraphQL type name for transport checkpoints.
const TRANSPORT_CHECKPOINT: &str = "TransportCheckpoint";

/// Render a changelog field/operation/argument identifier in the schema's naming
/// convention (#498).
///
/// The changelog's canonical identifiers are authored `snake_case` to match the
/// view's JSONB `data` keys and the upsert function's positional parameters. Under
/// [`NamingConvention::CamelCase`] they must render camelCase like every other
/// SDK-emitted type/field/operation, otherwise the injected surface is the only
/// `snake_case` corner of an otherwise camelCase API. The SQL contract is
/// unchanged: the runtime recovers the `snake_case` JSONB key via `to_snake_case`
/// (bijective with `to_camel_case`), and mutation arguments bind to the function
/// positionally — argument *names* never reach SQL. Type names
/// (`EntityChangeLog` / `TransportCheckpoint`) are `PascalCase` under both
/// conventions and are not passed through here.
///
/// This mirrors [`CompiledSchema::display_name`](super::CompiledSchema::display_name),
/// applied at injection time so the verbatim-rendering surfaces (the federation
/// `_service` SDL via `raw_schema()`, and object-field introspection) are
/// consistent too.
fn cased(name: &str, convention: NamingConvention) -> String {
    match convention {
        NamingConvention::CamelCase => to_camel_case(name),
        NamingConvention::Preserve => name.to_string(),
    }
}

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

    // GraphQL identifiers follow the schema-wide naming convention (#498); the
    // backing SQL (views/function) stays snake_case regardless.
    let nc = schema.naming_convention;

    schema.types.push(entity_change_log_type(&cfg, nc));
    schema.types.push(transport_checkpoint_type(&cfg, nc));
    schema.queries.push(entity_change_logs_query(&cfg, nc));
    schema.queries.push(transport_checkpoint_query(&cfg, nc));
    schema.mutations.push(upsert_checkpoint_mutation(&cfg, nc));

    schema.build_indexes();
}

/// `EntityChangeLog` — read-only projection over `{schema}.v_entity_change_log`.
///
/// Field identifiers follow `nc` (#498); the runtime maps them back to the view's
/// `snake_case` JSONB keys via `to_snake_case`.
fn entity_change_log_type(cfg: &ChangelogConfig, nc: NamingConvention) -> TypeDefinition {
    let cursor = cased("pk_entity_change_log", nc);
    let mut t =
        TypeDefinition::new(ENTITY_CHANGE_LOG, format!("{}.v_entity_change_log", cfg.schema))
            .with_description(format!(
                "An observer entity-change-log entry. Read-only; populated by the observer \
                 system. Paginate via the `{cursor}` cursor.",
            ))
            .with_field(FieldDefinition::new(cased("id", nc), FieldType::Id))
            .with_field(FieldDefinition::new(cursor, FieldType::Int))
            .with_field(FieldDefinition::nullable(cased("fk_customer_org", nc), FieldType::String))
            .with_field(FieldDefinition::nullable(cased("fk_contact", nc), FieldType::String))
            .with_field(FieldDefinition::new(cased("object_type", nc), FieldType::String))
            .with_field(FieldDefinition::new(cased("object_id", nc), FieldType::String))
            .with_field(FieldDefinition::new(cased("modification_type", nc), FieldType::String))
            .with_field(FieldDefinition::nullable(cased("change_status", nc), FieldType::String))
            .with_field(FieldDefinition::new(cased("object_data", nc), FieldType::Json))
            .with_field(FieldDefinition::nullable(cased("extra_metadata", nc), FieldType::Json))
            .with_field(FieldDefinition::new(cased("created_at", nc), FieldType::DateTime));
    t.requires_role.clone_from(&cfg.read_role);
    t
}

/// `TransportCheckpoint` — read-only projection over `{schema}.v_transport_checkpoint`.
fn transport_checkpoint_type(cfg: &ChangelogConfig, nc: NamingConvention) -> TypeDefinition {
    let key = cased("transport_name", nc);
    let mut t =
        TypeDefinition::new(TRANSPORT_CHECKPOINT, format!("{}.v_transport_checkpoint", cfg.schema))
            .with_description(format!(
                "A consumer's change-log cursor checkpoint, keyed by `{key}`.",
            ))
            .with_field(FieldDefinition::new(key, FieldType::String))
            .with_field(FieldDefinition::new(cased("last_pk", nc), FieldType::Int))
            .with_field(FieldDefinition::new(cased("updated_at", nc), FieldType::DateTime));
    t.requires_role.clone_from(&cfg.read_role);
    t
}

/// `entity_change_logs` — cursor-paginated list query using the generic filter machinery.
fn entity_change_logs_query(cfg: &ChangelogConfig, nc: NamingConvention) -> QueryDefinition {
    let cursor = cased("pk_entity_change_log", nc);
    let mut q = QueryDefinition::new(cased("entity_change_logs", nc), ENTITY_CHANGE_LOG)
        .returning_list()
        .with_sql_source(format!("{}.v_entity_change_log", cfg.schema));
    q.description = Some(format!(
        "Cursor-paginate the observer change-log. Poll with \
         `where: {{ {cursor}: {{ gt: $cursor }} }}`, \
         `orderBy: [{{ field: \"{cursor}\", direction: ASC }}]`, and a \
         `limit` (server max {}). Optional equality filters: {}, {}.",
        cfg.max_limit,
        cased("object_type", nc),
        cased("modification_type", nc),
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
fn transport_checkpoint_query(cfg: &ChangelogConfig, nc: NamingConvention) -> QueryDefinition {
    let key = cased("transport_name", nc);
    let mut q = QueryDefinition::new(cased("transport_checkpoint", nc), TRANSPORT_CHECKPOINT)
        .with_sql_source(format!("{}.v_transport_checkpoint", cfg.schema));
    q.nullable = true;
    q.description = Some(format!("Fetch one consumer's checkpoint by {key}."));
    // Named scalar arg → `WHERE data->>'transport_name' = $1` equality lookup; the
    // runtime recases the (possibly camelCase) argument name to the JSONB key.
    q.arguments = vec![ArgumentDefinition::new(key, FieldType::String)];
    q.cache_ttl_seconds = Some(0);
    q.requires_role.clone_from(&cfg.read_role);
    q
}

/// `upsert_transport_checkpoint(transport_name, last_pk)` — advance a consumer cursor.
fn upsert_checkpoint_mutation(cfg: &ChangelogConfig, nc: NamingConvention) -> MutationDefinition {
    let mut m =
        MutationDefinition::new(cased("upsert_transport_checkpoint", nc), TRANSPORT_CHECKPOINT);
    m.sql_source = Some(format!("{}.fn_upsert_transport_checkpoint", cfg.schema));
    m.description =
        Some("Create or advance a consumer's change-log checkpoint (idempotent).".to_string());
    // Arguments bind to the PG function positionally, so renaming them to camelCase
    // is safe — the function never sees the GraphQL argument names, only their order.
    m.arguments = vec![
        ArgumentDefinition::new(cased("transport_name", nc), FieldType::String),
        ArgumentDefinition::new(cased("last_pk", nc), FieldType::Int),
    ];
    m.requires_role.clone_from(&cfg.write_role);
    m
}

#[cfg(test)]
mod tests;
