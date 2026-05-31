//! Tests for `inject_changelog` schema-injection logic.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{ENTITY_CHANGE_LOG, TRANSPORT_CHECKPOINT, inject_changelog};
use crate::schema::{ChangelogConfig, CompiledSchema, FieldType};

fn exposed_schema(cfg: ChangelogConfig) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
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
