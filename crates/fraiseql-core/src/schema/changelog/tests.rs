//! Tests for `inject_changelog` schema-injection logic.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{ENTITY_CHANGE_LOG, TRANSPORT_CHECKPOINT, inject_changelog};
use crate::schema::{ChangelogConfig, CompiledSchema, FieldType, NamingConvention};

fn exposed_schema(cfg: ChangelogConfig) -> CompiledSchema {
    exposed_schema_with(cfg, NamingConvention::default())
}

fn exposed_schema_with(cfg: ChangelogConfig, naming: NamingConvention) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.naming_convention = naming;
    schema.changelog = Some(cfg);
    inject_changelog(&mut schema);
    schema
}

#[test]
fn disabled_config_injects_nothing() {
    let schema = exposed_schema(ChangelogConfig {
        expose: false,
        ..Default::default()
    });
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
    assert!(schema.mutations.is_empty());
}

#[test]
fn absent_config_injects_nothing() {
    let mut schema = CompiledSchema::new();
    inject_changelog(&mut schema);
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
}

#[test]
fn exposed_config_injects_full_surface() {
    let schema = exposed_schema(ChangelogConfig {
        expose: true,
        ..Default::default()
    });

    assert!(schema.types.iter().any(|t| t.name == ENTITY_CHANGE_LOG));
    assert!(schema.types.iter().any(|t| t.name == TRANSPORT_CHECKPOINT));
    assert!(schema.queries.iter().any(|q| q.name == "entity_change_logs"));
    assert!(schema.queries.iter().any(|q| q.name == "transport_checkpoint"));
    assert!(schema.mutations.iter().any(|m| m.name == "upsert_transport_checkpoint"));

    // Indexes were rebuilt → operations resolve in-memory.
    assert!(schema.query_index.contains_key("entity_change_logs"));
    assert!(schema.mutation_index.contains_key("upsert_transport_checkpoint"));
}

#[test]
fn list_query_uses_filter_machinery_and_bypasses_cache() {
    let schema = exposed_schema(ChangelogConfig {
        expose: true,
        ..Default::default()
    });
    let q = schema.queries.iter().find(|q| q.name == "entity_change_logs").unwrap();
    assert!(q.returns_list);
    assert!(q.auto_params.has_where);
    assert!(q.auto_params.has_order_by);
    assert!(q.auto_params.has_limit);
    assert_eq!(q.cache_ttl_seconds, Some(0), "real-time data must not be cached");
    assert_eq!(q.sql_source.as_deref(), Some("core.v_entity_change_log"));
}

#[test]
fn cursor_type_is_int_for_numeric_keyset() {
    // pk_entity_change_log MUST be Int so the runtime emits numeric casts for
    // `gt`/ORDER BY instead of lexicographic text comparison.
    let schema = exposed_schema(ChangelogConfig {
        expose: true,
        ..Default::default()
    });
    let t = schema.types.iter().find(|t| t.name == ENTITY_CHANGE_LOG).unwrap();
    let pk = t.find_field("pk_entity_change_log").unwrap();
    assert_eq!(pk.field_type, FieldType::Int);
    assert!(!pk.nullable);
}

#[test]
fn rbac_roles_propagate_from_config() {
    let cfg = ChangelogConfig {
        expose: true,
        read_role: Some("ops_reader".to_string()),
        write_role: Some("ops_writer".to_string()),
        ..Default::default()
    };
    let schema = exposed_schema(cfg);

    let list = schema.queries.iter().find(|q| q.name == "entity_change_logs").unwrap();
    assert_eq!(list.requires_role.as_deref(), Some("ops_reader"));
    let ty = schema.types.iter().find(|t| t.name == ENTITY_CHANGE_LOG).unwrap();
    assert_eq!(ty.requires_role.as_deref(), Some("ops_reader"));
    let upsert = schema
        .mutations
        .iter()
        .find(|m| m.name == "upsert_transport_checkpoint")
        .unwrap();
    assert_eq!(upsert.requires_role.as_deref(), Some("ops_writer"));
}

#[test]
fn schema_name_parameterizes_sql_sources() {
    let cfg = ChangelogConfig {
        expose: true,
        schema: "audit".to_string(),
        ..Default::default()
    };
    let schema = exposed_schema(cfg);
    let ty = schema.types.iter().find(|t| t.name == ENTITY_CHANGE_LOG).unwrap();
    assert_eq!(ty.sql_source.as_str(), "audit.v_entity_change_log");
    let upsert = schema
        .mutations
        .iter()
        .find(|m| m.name == "upsert_transport_checkpoint")
        .unwrap();
    assert_eq!(upsert.sql_source.as_deref(), Some("audit.fn_upsert_transport_checkpoint"));
}

// --- #498: injected identifiers honour the schema-wide naming convention. ------

/// Under the default `Preserve` convention the GraphQL-facing identifiers stay
/// `snake_case` (matching the authored form and the view's JSONB `data` keys).
#[test]
fn preserve_convention_keeps_snake_case_identifiers() {
    let schema = exposed_schema_with(
        ChangelogConfig {
            expose: true,
            ..Default::default()
        },
        NamingConvention::Preserve,
    );

    let ecl = schema.types.iter().find(|t| t.name == ENTITY_CHANGE_LOG).unwrap();
    assert!(ecl.find_field("pk_entity_change_log").is_some());
    assert!(ecl.find_field("object_type").is_some());
    assert!(schema.queries.iter().any(|q| q.name == "entity_change_logs"));
    assert!(schema.queries.iter().any(|q| q.name == "transport_checkpoint"));
    assert!(schema.mutations.iter().any(|m| m.name == "upsert_transport_checkpoint"));
}

/// Under `camelCase` every changelog field/operation/argument renders camelCase
/// like the rest of the SDK-emitted surface (#498). Type names stay `PascalCase`.
#[test]
fn camel_case_convention_renders_camelcase_identifiers() {
    let schema = exposed_schema_with(
        ChangelogConfig {
            expose: true,
            ..Default::default()
        },
        NamingConvention::CamelCase,
    );

    // Type names are PascalCase under both conventions.
    let ecl = schema.types.iter().find(|t| t.name == ENTITY_CHANGE_LOG).unwrap();
    let chk = schema.types.iter().find(|t| t.name == TRANSPORT_CHECKPOINT).unwrap();

    // Object fields are camelCase; the snake_case forms are gone.
    for camel in [
        "pkEntityChangeLog",
        "fkCustomerOrg",
        "fkContact",
        "objectType",
        "objectId",
        "modificationType",
        "changeStatus",
        "objectData",
        "extraMetadata",
        "createdAt",
    ] {
        assert!(ecl.find_field(camel).is_some(), "missing camelCase field {camel}");
    }
    assert!(ecl.find_field("id").is_some(), "single-word field stays as-is");
    assert!(ecl.find_field("pk_entity_change_log").is_none(), "snake_case field leaked");
    assert!(ecl.find_field("object_type").is_none(), "snake_case field leaked");

    for camel in ["transportName", "lastPk", "updatedAt"] {
        assert!(chk.find_field(camel).is_some(), "missing camelCase field {camel}");
    }
    assert!(chk.find_field("transport_name").is_none(), "snake_case field leaked");

    // Operation names are camelCase.
    assert!(schema.queries.iter().any(|q| q.name == "entityChangeLogs"));
    assert!(schema.queries.iter().any(|q| q.name == "transportCheckpoint"));
    assert!(schema.mutations.iter().any(|m| m.name == "upsertTransportCheckpoint"));
    assert!(!schema.queries.iter().any(|q| q.name == "entity_change_logs"));

    // Argument names are camelCase.
    let lookup = schema.queries.iter().find(|q| q.name == "transportCheckpoint").unwrap();
    assert_eq!(lookup.arguments.len(), 1);
    assert_eq!(lookup.arguments[0].name, "transportName");

    let upsert = schema.mutations.iter().find(|m| m.name == "upsertTransportCheckpoint").unwrap();
    let arg_names: Vec<&str> = upsert.arguments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(arg_names, vec!["transportName", "lastPk"]);
}

/// The camelCase identifiers round-trip back to the exact `snake_case` JSONB keys
/// the views expose, so the SQL contract is unchanged — only the GraphQL surface
/// is recased (#498).
#[test]
fn camel_case_fields_round_trip_to_snake_jsonb_keys() {
    use crate::utils::to_snake_case;

    assert_eq!(to_snake_case("pkEntityChangeLog"), "pk_entity_change_log");
    assert_eq!(to_snake_case("fkCustomerOrg"), "fk_customer_org");
    assert_eq!(to_snake_case("objectType"), "object_type");
    assert_eq!(to_snake_case("changeStatus"), "change_status");
    assert_eq!(to_snake_case("extraMetadata"), "extra_metadata");
    assert_eq!(to_snake_case("transportName"), "transport_name");
    assert_eq!(to_snake_case("lastPk"), "last_pk");
}

/// The federation `_service` SDL (built from `raw_schema()`, which renders names
/// verbatim) is the surface where the `snake_case` leak was first observed (#498).
/// Under `camelCase` the rendered changelog surface must be camelCase end-to-end.
#[test]
fn federation_sdl_renders_changelog_camelcase() {
    let schema = exposed_schema_with(
        ChangelogConfig {
            expose: true,
            ..Default::default()
        },
        NamingConvention::CamelCase,
    );
    let sdl = schema.raw_schema();

    // Object fields + root operations + arguments render camelCase. The list
    // query's filter args come from `auto_params`, not `ArgumentDefinition`, so it
    // renders without parentheses (`entityChangeLogs: [EntityChangeLog!]!`).
    assert!(sdl.contains("pkEntityChangeLog"), "SDL missing camelCase field");
    assert!(sdl.contains("objectType"), "SDL missing camelCase field");
    assert!(sdl.contains("entityChangeLogs"), "SDL missing camelCase query");
    assert!(sdl.contains("transportName"), "SDL missing camelCase argument");
    assert!(sdl.contains("upsertTransportCheckpoint("), "SDL missing camelCase mutation");

    // No snake_case identifiers leak into the rendered SDL.
    assert!(!sdl.contains("pk_entity_change_log"), "snake_case field leaked into SDL");
    assert!(!sdl.contains("object_type"), "snake_case field leaked into SDL");
    assert!(!sdl.contains("entity_change_logs"), "snake_case query leaked into SDL");
    assert!(!sdl.contains("transport_name"), "snake_case argument leaked into SDL");
}
