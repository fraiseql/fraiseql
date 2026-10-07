//! Live-PostgreSQL integration tests for the opt-in fail-fast `sql_source` boot
//! check (#487), and the cross-crate guarantee that the server boot check and the
//! CLI `validate --against-db` gate agree on "backed".
//!
//! Self-skips when no `DATABASE_URL` is set.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code

use std::collections::BTreeSet;

use fraiseql_cli::schema::database_validator::{
    create_introspector, find_unbacked_sources as cli_find_unbacked,
};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{CompiledSchema, MutationDefinition, QueryDefinition, SourceProbe},
};
use fraiseql_server::sql_source_check::find_unbacked_sources as server_find_unbacked;
use fraiseql_test_utils::try_database_url;
use tokio_postgres::NoTls;

const SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_487_test CASCADE;
CREATE SCHEMA fql_487_test;
CREATE VIEW fql_487_test.v_orders AS SELECT '{}'::jsonb AS data;
CREATE FUNCTION fql_487_test.fn_create_order(p_input jsonb)
  RETURNS jsonb LANGUAGE sql AS $$ SELECT p_input $$;
";

const TEARDOWN: &str = "DROP SCHEMA IF EXISTS fql_487_test CASCADE;";

async fn run_sql(url: &str, sql: &str) {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(sql).await.unwrap();
}

fn query(name: &str, sql_source: &str) -> QueryDefinition {
    QueryDefinition::new(name, "T").with_sql_source(sql_source).returning_list()
}

fn mutation(name: &str, sql_source: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "T");
    m.sql_source = Some(sql_source.to_string());
    m
}

/// Render the unbacked probes as a comparable set of `display_name` strings.
fn names(probes: &[SourceProbe]) -> BTreeSet<String> {
    probes.iter().map(SourceProbe::display_name).collect()
}

fn fully_backed_schema() -> CompiledSchema {
    CompiledSchema {
        queries: vec![query("orders", "fql_487_test.v_orders")],
        mutations: vec![mutation("createOrder", "fql_487_test.fn_create_order")],
        ..Default::default()
    }
}

fn schema_with_two_missing() -> CompiledSchema {
    CompiledSchema {
        queries: vec![
            query("orders", "fql_487_test.v_orders"),
            query("missing", "fql_487_test.v_missing"),
        ],
        mutations: vec![
            mutation("createOrder", "fql_487_test.fn_create_order"),
            mutation("absent", "fql_487_test.fn_absent"),
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn boot_check_passes_when_all_sources_backed() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #487 boot-check test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let unbacked = server_find_unbacked(&fully_backed_schema(), &adapter).await.unwrap();
    assert!(unbacked.is_empty(), "fully-backed schema must boot clean, got {unbacked:?}");

    run_sql(&url, TEARDOWN).await;
}

#[tokio::test]
async fn boot_check_lists_each_unbacked_source() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #487 boot-check test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let unbacked = server_find_unbacked(&schema_with_two_missing(), &adapter).await.unwrap();
    assert_eq!(
        names(&unbacked),
        BTreeSet::from([
            "fql_487_test.v_missing".to_string(),
            "fql_487_test.fn_absent".to_string(),
        ]),
        "exactly the missing view + function must be reported",
    );

    run_sql(&url, TEARDOWN).await;
}

/// The point of the shared `sql_source_probes` core: the server boot check (via the
/// adapter) and the CLI `validate --against-db` gate (via the introspector) must
/// report the **same** unbacked set on the same database.
#[tokio::test]
async fn server_and_cli_agree_on_unbacked_set() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #487 symmetry test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let schema = schema_with_two_missing();

    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let server_set = names(&server_find_unbacked(&schema, &adapter).await.unwrap());

    let introspector =
        create_introspector(&url, &fraiseql_core::db::postgres::PostgresTlsConfig::default())
            .await
            .unwrap();
    let cli_set = names(&cli_find_unbacked(&schema, &introspector).await.unwrap());

    assert_eq!(server_set, cli_set, "server boot check and CLI gate must agree on 'backed'");

    run_sql(&url, TEARDOWN).await;
}

// ── #1426: a source the server's role may not use ─────────────────────────────

const ROLE: &str = "fql_1426_api";

const PRIVILEGE_SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_1426_test CASCADE;
DROP SCHEMA IF EXISTS fql_1426_hidden CASCADE;
DROP ROLE IF EXISTS fql_1426_api;
CREATE ROLE fql_1426_api LOGIN PASSWORD 'fql_1426_api';
CREATE SCHEMA fql_1426_test;
GRANT USAGE ON SCHEMA fql_1426_test TO fql_1426_api;
CREATE VIEW fql_1426_test.v_granted AS SELECT '{}'::jsonb AS data;
GRANT SELECT ON fql_1426_test.v_granted TO fql_1426_api;
CREATE VIEW fql_1426_test.v_ungranted AS SELECT '{}'::jsonb AS data;
CREATE FUNCTION fql_1426_test.fn_granted(p_input jsonb)
  RETURNS jsonb LANGUAGE sql AS $$ SELECT p_input $$;
CREATE FUNCTION fql_1426_test.fn_revoked(p_input jsonb)
  RETURNS jsonb LANGUAGE sql AS $$ SELECT p_input $$;
REVOKE EXECUTE ON FUNCTION fql_1426_test.fn_revoked(jsonb) FROM PUBLIC;
CREATE SCHEMA fql_1426_hidden;
CREATE VIEW fql_1426_hidden.v_orders AS SELECT '{}'::jsonb AS data;
";

const PRIVILEGE_TEARDOWN: &str = "\
DROP SCHEMA IF EXISTS fql_1426_test CASCADE;
DROP SCHEMA IF EXISTS fql_1426_hidden CASCADE;
DROP ROLE IF EXISTS fql_1426_api;
";

/// `DATABASE_URL` with the credentials of the under-privileged role.
fn as_role(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap();
    let host = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    format!("{scheme}://{ROLE}:{ROLE}@{host}")
}

fn schema_with_privilege_gaps() -> CompiledSchema {
    CompiledSchema {
        queries: vec![
            query("granted", "fql_1426_test.v_granted"),
            query("ungranted", "fql_1426_test.v_ungranted"),
            query("hidden", "fql_1426_hidden.v_orders"),
        ],
        mutations: vec![
            mutation("granted", "fql_1426_test.fn_granted"),
            mutation("revoked", "fql_1426_test.fn_revoked"),
        ],
        ..Default::default()
    }
}

/// Every source exists, so the existence check passes; as the server's role, three of
/// them cannot be used, and each is reported with the privilege it lacks. The view in a
/// schema without USAGE used to fail the existence probe itself with "permission denied
/// for schema" instead of being reported.
#[tokio::test]
async fn boot_check_reports_each_source_the_role_may_not_use() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #1426 boot-check test: no DATABASE_URL");
        return;
    };
    run_sql(&url, PRIVILEGE_SETUP).await;

    let adapter = PostgresAdapter::new(&as_role(&url)).await.unwrap();
    let schema = schema_with_privilege_gaps();
    let unbacked = server_find_unbacked(&schema, &adapter).await;
    let unusable =
        fraiseql_server::sql_source_check::find_unusable_sources(&schema, &adapter).await;

    run_sql(&url, PRIVILEGE_TEARDOWN).await;
    assert!(unbacked.unwrap().is_empty(), "every source exists");
    let unusable: BTreeSet<(String, String)> = unusable
        .unwrap()
        .into_iter()
        .map(|(probe, missing)| (probe.display_name(), missing))
        .collect();
    assert_eq!(
        unusable,
        BTreeSet::from([
            ("fql_1426_test.v_ungranted".to_string(), "SELECT".to_string()),
            ("fql_1426_test.fn_revoked".to_string(), "EXECUTE".to_string()),
            (
                "fql_1426_hidden.v_orders".to_string(),
                "USAGE on schema fql_1426_hidden".to_string()
            ),
        ])
    );
}

/// The boot message names the role and each missing privilege beside any missing source.
#[test]
fn the_boot_message_names_the_role_and_each_missing_privilege() {
    use fraiseql_core::schema::SourceKind;
    let probe = |kind, name: &str| SourceProbe {
        kind,
        schema: Some("app".to_string()),
        name: name.to_string(),
    };
    let message = fraiseql_server::sql_source_check::format_source_problems(
        &[probe(SourceKind::Relation, "v_missing")],
        &[(probe(SourceKind::Function, "create_order"), "EXECUTE".to_string())],
        "api_role",
    );
    assert!(message.contains("app.v_missing (relation) does not exist"), "{message}");
    assert!(
        message.contains("app.create_order (function): EXECUTE not granted to api_role"),
        "{message}"
    );
}
