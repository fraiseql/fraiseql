//! Unit tests for the `sql_source` probe-list builder (no database).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;
use crate::schema::{MutationDefinition, MutationOperation, QueryDefinition};

fn query(name: &str, sql_source: Option<&str>) -> QueryDefinition {
    let mut q = QueryDefinition::new(name, "T");
    q.sql_source = sql_source.map(str::to_string);
    q
}

fn mutation(name: &str, sql_source: Option<&str>, op: MutationOperation) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "T");
    m.sql_source = sql_source.map(str::to_string);
    m.operation = op;
    m
}

fn schema_with(
    queries: Vec<QueryDefinition>,
    mutations: Vec<MutationDefinition>,
) -> CompiledSchema {
    CompiledSchema {
        queries,
        mutations,
        ..Default::default()
    }
}

#[test]
fn query_relation_probe_bare_name() {
    let s = schema_with(vec![query("orders", Some("v_orders"))], vec![]);
    let probes = sql_source_probes(&s);
    assert_eq!(
        probes,
        vec![SourceProbe {
            schema: None,
            name:   "v_orders".to_string(),
            kind:   SourceKind::Relation,
        }]
    );
}

#[test]
fn query_relation_probe_schema_qualified() {
    let s = schema_with(vec![query("changes", Some("events.v_change_log"))], vec![]);
    let probes = sql_source_probes(&s);
    assert_eq!(
        probes,
        vec![SourceProbe {
            schema: Some("events".to_string()),
            name:   "v_change_log".to_string(),
            kind:   SourceKind::Relation,
        }]
    );
}

#[test]
fn mutation_function_probe_schema_qualified() {
    let s = schema_with(
        vec![],
        vec![mutation(
            "createOrder",
            Some("app.create_order"),
            MutationOperation::default(),
        )],
    );
    let probes = sql_source_probes(&s);
    assert_eq!(
        probes,
        vec![SourceProbe {
            schema: Some("app".to_string()),
            name:   "create_order".to_string(),
            kind:   SourceKind::Function,
        }]
    );
}

#[test]
fn mixed_case_identifiers_are_kept_verbatim() {
    // The runtime resolves identifiers verbatim via quote_postgres_identifier, so
    // the probe is case-sensitive — no folding to lowercase.
    let s = schema_with(vec![query("foo", Some("events.V_Orders"))], vec![]);
    let probes = sql_source_probes(&s);
    assert_eq!(probes[0].schema.as_deref(), Some("events"));
    assert_eq!(probes[0].name, "V_Orders");
}

#[test]
fn operations_without_sql_source_are_skipped() {
    // A federation/non-SQL query and a Custom mutation with no source contribute
    // no probe.
    let s = schema_with(
        vec![query("federated", None)],
        vec![mutation("notify", None, MutationOperation::Custom)],
    );
    assert!(sql_source_probes(&s).is_empty());
}

#[test]
fn mutation_falls_back_to_operation_table_when_sql_source_absent() {
    // Mirrors the #397 resolve_sql_source fallback: an Insert with a non-empty
    // table but no explicit sql_source still names a function to probe.
    let s = schema_with(
        vec![],
        vec![mutation(
            "createUser",
            None,
            MutationOperation::Insert {
                table: "app.fn_create_user".to_string(),
            },
        )],
    );
    let probes = sql_source_probes(&s);
    assert_eq!(
        probes,
        vec![SourceProbe {
            schema: Some("app".to_string()),
            name:   "fn_create_user".to_string(),
            kind:   SourceKind::Function,
        }]
    );
}

#[test]
fn display_name_round_trips_qualification() {
    let qualified = SourceProbe {
        schema: Some("app".to_string()),
        name:   "f".to_string(),
        kind:   SourceKind::Function,
    };
    let bare = SourceProbe {
        schema: None,
        name:   "v".to_string(),
        kind:   SourceKind::Relation,
    };
    assert_eq!(qualified.display_name(), "app.f");
    assert_eq!(bare.display_name(), "v");
}
