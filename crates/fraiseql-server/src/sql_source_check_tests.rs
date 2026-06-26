//! Unit tests for the boot-time `sql_source` existence-probe SQL builders.

#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_core::schema::{SourceKind, SourceProbe};

use super::{existence_sql, format_unbacked};

fn relation(schema: Option<&str>, name: &str) -> SourceProbe {
    SourceProbe {
        schema: schema.map(str::to_string),
        name:   name.to_string(),
        kind:   SourceKind::Relation,
    }
}

fn function(schema: Option<&str>, name: &str) -> SourceProbe {
    SourceProbe {
        schema: schema.map(str::to_string),
        name:   name.to_string(),
        kind:   SourceKind::Function,
    }
}

#[test]
fn qualified_relation_uses_quoted_to_regclass() {
    // Verbatim, case-sensitive — exactly how the runtime resolves it.
    let sql = existence_sql(&relation(Some("events"), "V_Orders"));
    assert!(sql.contains(r#"to_regclass('"events"."V_Orders"')"#), "got: {sql}");
    assert!(sql.contains("AS source_exists"), "got: {sql}");
}

#[test]
fn bare_relation_quotes_the_name() {
    // Bare name → search_path resolution, but still quoted (verbatim).
    let sql = existence_sql(&relation(None, "v_orders"));
    assert!(sql.contains(r#"to_regclass('"v_orders"')"#), "got: {sql}");
}

#[test]
fn qualified_function_probes_pg_proc_by_name_and_schema() {
    let sql = existence_sql(&function(Some("app"), "create_order"));
    assert!(sql.contains("n.nspname = 'app'"), "got: {sql}");
    assert!(sql.contains("p.proname = 'create_order'"), "got: {sql}");
    assert!(sql.contains("p.prokind IN ('f','p')"), "got: {sql}");
}

#[test]
fn bare_function_scopes_to_current_schemas() {
    let sql = existence_sql(&function(None, "create_order"));
    assert!(sql.contains("current_schemas(false)"), "got: {sql}");
    assert!(sql.contains("p.proname = 'create_order'"), "got: {sql}");
}

#[test]
fn single_quotes_in_identifiers_are_escaped() {
    // Defensive: a single quote in a name must not break out of the literal.
    let sql = existence_sql(&function(None, "o'brien"));
    assert!(sql.contains("p.proname = 'o''brien'"), "got: {sql}");
}

#[test]
fn format_unbacked_lists_each_source_with_kind() {
    let out = format_unbacked(&[
        relation(Some("events"), "v_missing"),
        function(None, "fn_absent"),
    ]);
    assert!(out.contains("events.v_missing (relation) does not exist"), "got: {out}");
    assert!(out.contains("fn_absent (function) does not exist"), "got: {out}");
}
