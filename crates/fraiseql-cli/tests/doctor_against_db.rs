//! Live-`PostgreSQL` integration tests for the `doctor --against-db` PL/pgSQL
//! body-resolution pass (#409).
//!
//! The body-resolution pass depends on the `plpgsql_check` extension, which is
//! absent on stock Postgres images (including the CI image) and most managed
//! services. These tests assert the **graceful-degradation** contract: when the
//! extension is unavailable the pass reports
//! [`PlpgsqlCheckOutcome::Unavailable`] instead of erroring, and when it is
//! available it runs. The "found an unresolved call" happy path requires the
//! extension and is therefore not exercised here.
//!
//! Self-skips when no `DATABASE_URL` is set.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code — panics and skip diagnostics are acceptable

use std::io::Write;

use fraiseql_cli::{
    commands::doctor::{CheckStatus, effective_db_url, run_checks},
    schema::pg_catalog::{PgCatalog, PlpgsqlCheckOutcome},
};
use tempfile::Builder;

/// The three change-log relations the PUBLIC-grants check inspects.
const CHANGE_LOG_RELATIONS: [&str; 3] = [
    "tb_entity_change_log",
    "v_entity_change_log",
    "v_entity_change_log_debezium",
];

async fn catalog() -> Option<PgCatalog> {
    let url = fraiseql_test_support::try_database_url()?;
    match PgCatalog::connect(&url, &fraiseql_db::postgres::PostgresTlsConfig::default()).await {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("skipping #409 against-db test: {e}");
            None
        },
    }
}

/// #488 end-to-end: the common Python-SDK flow — `doctor --against-db <url>
/// --schema <compiled.json>` with a runtime-config `fraiseql.toml` — must produce
/// no spurious connectivity or TOML-schema reds. Drives `run_checks` with the
/// effective URL (no `--db-url`), a compiled-JSON schema, and a runtime config.
#[tokio::test]
async fn doctor_against_db_has_no_spurious_connectivity_or_toml_reds() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #488 doctor against-db test: no DATABASE_URL");
        return;
    };

    let mut config = Builder::new().suffix(".toml").tempfile().unwrap();
    config.write_all(b"[server]\nbind = \"0.0.0.0:8000\"\n").unwrap();
    config.flush().unwrap();
    let mut schema = Builder::new().suffix(".json").tempfile().unwrap();
    schema
        .write_all(br#"{"version":1,"types":[],"queries":[],"mutations":[]}"#)
        .unwrap();
    schema.flush().unwrap();

    // Connectivity uses the effective URL (`--against-db` when no `--db-url`).
    let checks = run_checks(config.path(), schema.path(), effective_db_url(None, Some(&url)));

    let check = |name: &str| {
        checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing check `{name}`"))
    };

    // The `--against-db` URL is threaded: the "set" check passes (not "not set").
    assert_eq!(
        check("DATABASE_URL set").status,
        CheckStatus::Pass,
        "connectivity must honour --against-db: {checks:?}"
    );
    // The reachability check now attempts a connect against the threaded URL
    // rather than reporting "not set" (the #488 bug). Whether the TCP connect
    // itself succeeds depends on the URL host being IP-parseable, which is an
    // orthogonal pre-existing limitation — so assert it stopped saying "not set".
    assert!(
        !check("DATABASE_URL reachable").detail.contains("not set"),
        "reachable check must use the --against-db URL, not env: {checks:?}"
    );
    // A runtime-config TOML must be syntax-checked, not schema-parsed.
    assert_ne!(
        check("config TOML syntax valid").status,
        CheckStatus::Fail,
        "a runtime-config TOML must not be schema-parsed: {checks:?}"
    );
}

#[tokio::test]
async fn body_resolution_degrades_gracefully_when_extension_absent() {
    let Some(catalog) = catalog().await else {
        return;
    };

    // Probe availability, then run the pass; the two must agree and the pass
    // must never error out (the whole point of #409's degradation path).
    let available = match catalog.plpgsql_check_available().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: cannot probe extensions ({e})");
            return;
        },
    };

    let outcome = catalog
        .plpgsql_check_unresolved_calls(&["public".to_string()])
        .await
        .expect("body-resolution pass must not error");

    if available {
        assert!(
            matches!(outcome, PlpgsqlCheckOutcome::Ran { .. }),
            "extension available → pass should run"
        );
    } else {
        assert_eq!(
            outcome,
            PlpgsqlCheckOutcome::Unavailable,
            "extension absent → pass should skip gracefully"
        );
    }
}

#[tokio::test]
async fn non_postgres_url_is_rejected() {
    // PgCatalog::connect rejects non-postgres URLs up front (the --against-db
    // checks are PostgreSQL-only).
    assert!(
        PgCatalog::connect(
            "mysql://localhost/db",
            &fraiseql_db::postgres::PostgresTlsConfig::default()
        )
        .await
        .is_err(),
        "non-postgres URL rejected"
    );
}

/// `table_columns` reads column names + `udt_name` for the exact `PostgreSQL`
/// base types the change-log contract drift check (#380) compares against.
///
/// Uses a uniquely-named probe table in `public` (created + dropped here) so it
/// never touches the shared `core.tb_entity_change_log` other suites depend on.
#[tokio::test]
async fn table_columns_reads_name_and_udt() {
    const PROBE: &str = "public.tb_changelog_drift_probe_380";

    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };

    // Setup/teardown DDL goes over a raw connection — PgCatalog is read-only.
    let (client, conn) = match tokio_postgres::connect(&url, tokio_postgres::NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #380 table_columns test: {e}");
            return;
        },
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    client
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {PROBE}; \
             CREATE TABLE {PROBE} ( \
                object_id uuid, \
                label     text, \
                fk_thing  bigint, \
                tags      text[] \
             );"
        ))
        .await
        .expect("create probe table");

    let catalog = PgCatalog::connect(&url, &fraiseql_db::postgres::PostgresTlsConfig::default())
        .await
        .expect("connect catalog");
    let cols = catalog
        .table_columns("public", "tb_changelog_drift_probe_380")
        .await
        .expect("introspect probe table");

    // Drop before asserting so a failed assertion still leaves a clean DB.
    client
        .batch_execute(&format!("DROP TABLE IF EXISTS {PROBE};"))
        .await
        .expect("drop probe table");

    let by_name: std::collections::HashMap<&str, &str> =
        cols.iter().map(|c| (c.name.as_str(), c.udt_name.as_str())).collect();
    assert_eq!(cols.len(), 4, "all four probe columns are read: {cols:?}");
    assert_eq!(by_name.get("object_id"), Some(&"uuid"), "uuid → udt_name uuid");
    assert_eq!(by_name.get("label"), Some(&"text"), "text → udt_name text");
    assert_eq!(by_name.get("fk_thing"), Some(&"int8"), "bigint → udt_name int8");
    assert_eq!(by_name.get("tags"), Some(&"_text"), "text[] → udt_name _text");
}

/// An absent table introspects to an empty column list — the drift check reads
/// that as "table not found, the migration will install it".
#[tokio::test]
async fn table_columns_absent_table_is_empty() {
    let Some(catalog) = catalog().await else {
        return;
    };
    let cols = catalog
        .table_columns("core", "tb_a_table_that_does_not_exist_380")
        .await
        .expect("introspection of an absent table must not error");
    assert!(cols.is_empty(), "absent table → empty column list, got: {cols:?}");
}

/// `change_log_public_grants` runs against real `PostgreSQL` (`aclexplode` over
/// `pg_class.relacl`) and, for any change-log relation that exists, reports the
/// PUBLIC privileges. The shipped migration 12 `REVOKE ALL … FROM PUBLIC` means a
/// present relation must show **no** PUBLIC privileges (#443); an absent relation
/// is simply omitted.
#[tokio::test]
async fn change_log_public_grants_reads_clean_baseline() {
    let Some(catalog) = catalog().await else {
        return;
    };
    let grants = catalog
        .change_log_public_grants()
        .await
        .expect("change-log PUBLIC-grants introspection must not error");

    for g in &grants {
        assert!(
            CHANGE_LOG_RELATIONS.contains(&g.relname.as_str()),
            "only the three change-log relations are reported, got: {}",
            g.relname
        );
        assert!(
            g.privileges.is_empty(),
            "migration 12 REVOKEs all PUBLIC privileges; `core.{}` still grants {:?}",
            g.relname,
            g.privileges
        );
    }
}

/// `capture_fn_security` runs against real `PostgreSQL` (`pg_proc.prosecdef` +
/// `proconfig`) and, when `core.fn_entity_change_log_capture()` exists, reports it
/// as `SECURITY DEFINER` with a pinned `search_path` (the migration-11 posture,
/// #443 / #437 F6). An absent function introspects to `None` without erroring.
#[tokio::test]
async fn capture_fn_security_reads_definer_and_pinned_search_path() {
    let Some(catalog) = catalog().await else {
        return;
    };
    let status = catalog
        .capture_fn_security()
        .await
        .expect("capture-function security introspection must not error");

    if let Some(s) = status {
        assert!(s.security_definer, "the capture function must be SECURITY DEFINER");
        assert!(
            s.search_path.as_deref().is_some_and(|sp| sp.contains("core")),
            "the capture function must pin a search_path including `core`, got: {:?}",
            s.search_path
        );
    }
}

/// `change_log_actor_stats` runs against real `PostgreSQL` and can actually SEE
/// what it reports (#390): NULL-actor rows are counted, a canonical token is
/// never classified as unknown, an out-of-contract token IS classified as
/// unknown, and the `chk_entity_change_log_actor_type` constraint presence is
/// read back. The scenario briefly drops the constraint to plant the rogue row
/// (the constraint would otherwise refuse it — which is the point of the
/// constraint) and restores the contract posture by re-applying the idempotent
/// contract DDL before asserting.
#[tokio::test]
async fn change_log_actor_stats_sees_null_unknown_and_constraint() {
    const MARKER: &str = "ActorStatsProbe390";
    const ROGUE: &str = "rogue_probe_390";
    // Compile-time dependency on the vendored contract DDL (byte-identical to
    // observers migration 08 by the setup lockstep test).
    const CONTRACT_SQL: &str = include_str!("../sql/helpers/entity_change_log_contract.sql");

    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    let (client, conn) = match tokio_postgres::connect(&url, tokio_postgres::NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #390 actor-stats test: {e}");
            return;
        },
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Bring the shared table to the current contract via the ONE shared
    // provisioner (#942/#982): drop-and-recreate, because the additive contract
    // cannot undo a stricter twin an earlier suite left behind (the warm-database
    // red this issue documented).
    client
        .batch_execute(&fraiseql_test_support::changelog::entity_change_log_provision_sql())
        .await
        .expect("apply contract DDL");

    // Plant: two unattributed rows, one canonical row, and — with the
    // constraint temporarily out of the way — one rogue-token row.
    client
        .batch_execute(&format!(
            "DELETE FROM core.tb_entity_change_log WHERE object_type = '{MARKER}'; \
             INSERT INTO core.tb_entity_change_log (object_type, modification_type, actor_type) \
             VALUES ('{MARKER}', 'INSERT', NULL), \
                    ('{MARKER}', 'INSERT', NULL), \
                    ('{MARKER}', 'INSERT', 'human_user'); \
             ALTER TABLE core.tb_entity_change_log \
                 DROP CONSTRAINT IF EXISTS chk_entity_change_log_actor_type; \
             INSERT INTO core.tb_entity_change_log (object_type, modification_type, actor_type) \
             VALUES ('{MARKER}', 'INSERT', '{ROGUE}');"
        ))
        .await
        .expect("plant actor-stats probe rows");

    let catalog = PgCatalog::connect(&url, &fraiseql_db::postgres::PostgresTlsConfig::default())
        .await
        .expect("connect catalog");
    let tokens: Vec<String> = fraiseql_core::security::ActorType::ALL
        .iter()
        .map(|a| a.as_str().to_string())
        .collect();
    let stats_without_constraint = catalog.change_log_actor_stats(&tokens).await;

    // Restore the contract posture (re-adds the constraint, NOT VALID) and
    // remove the probe rows BEFORE asserting, so a failed assertion still
    // leaves the shared table clean.
    client.batch_execute(CONTRACT_SQL).await.expect("re-apply contract DDL");
    client
        .batch_execute(&format!(
            "DELETE FROM core.tb_entity_change_log WHERE object_type = '{MARKER}';"
        ))
        .await
        .expect("remove probe rows");
    let stats_restored = catalog.change_log_actor_stats(&tokens).await;

    let s = stats_without_constraint
        .expect("actor-stats introspection must not error")
        .expect("table exists — stats must be Some");
    assert!(s.null_rows >= 2, "the two planted NULL-actor rows are counted: {s:?}");
    assert!(
        s.unknown_values.iter().any(|v| v == ROGUE),
        "the planted rogue token is classified as unknown: {s:?}"
    );
    assert!(
        !s.unknown_values.iter().any(|v| v == "human_user"),
        "a canonical token is never classified as unknown: {s:?}"
    );
    assert!(!s.constraint_installed, "constraint was dropped for the plant: {s:?}");

    let s = stats_restored
        .expect("actor-stats introspection must not error after restore")
        .expect("table exists — stats must be Some");
    assert!(s.constraint_installed, "re-applying the contract DDL restores the constraint");
    assert!(
        !s.unknown_values.iter().any(|v| v == ROGUE),
        "probe rows removed — the rogue token is gone: {s:?}"
    );
}

/// #1513: `doctor --against-db` names each (localized field, allowed locale) whose filter and
/// sort no index serves, judged by the plan. An index on the same key under another name
/// counts; with all present the check passes. The table holds three rows, as a fresh
/// deployment's does, so a planner left to choose scans it: the check must price the scan out
/// to see the index.
#[tokio::test]
async fn doctor_names_each_missing_localized_index() {
    use fraiseql_cli::commands::doctor::localized_index_checks;

    const TABLE: &str = "tv_doctor_locale_product";
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1513 doctor test: no DATABASE_URL");
        return;
    };
    let schema = localized_doctor_schema(TABLE);
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {TABLE}; CREATE TABLE {TABLE} (pk bigint, data jsonb); \
             INSERT INTO {TABLE} SELECT i, jsonb_build_object('name', jsonb_build_object('fr-FR', \
             'Nom ' || i)) FROM generate_series(1, 3) AS i; ANALYZE {TABLE};"
        ))
        .await
        .unwrap();
    let report = schema.localized_index_report();
    let fr = report.iter().find(|a| a.locale == "fr-FR").unwrap().index.clone().unwrap();
    // The fr-FR index, under a name of the operator's own: judged by plan, not by name.
    client
        .batch_execute(&fr.ddl.replace(&fr.name, "operator_named_fr_index"))
        .await
        .unwrap();
    client.batch_execute(&format!("ANALYZE {TABLE}")).await.unwrap();

    let tls = fraiseql_db::postgres::PostgresTlsConfig::default();
    // `doctor --against-db` runs the check, from the compiled schema file.
    let mut file = Builder::new().suffix(".json").tempfile().unwrap();
    file.write_all(serde_json::to_string(&schema).unwrap().as_bytes()).unwrap();
    file.flush().unwrap();
    let from_doctor: Vec<String> =
        fraiseql_cli::commands::doctor::against_db_checks(&url, &tls, file.path(), &[])
            .await
            .into_iter()
            .filter(|c| c.name == "Localized index" && c.status == CheckStatus::Warn)
            .map(|c| c.detail)
            .collect();
    let missing: Vec<String> = localized_index_checks(&url, &tls, &schema)
        .await
        .into_iter()
        .filter(|c| c.status == CheckStatus::Warn)
        .map(|c| c.detail)
        .collect();
    assert_eq!(missing.len(), 2, "{missing:?}");
    assert!(missing.iter().any(|d| d.contains("[en-US]")), "{missing:?}");
    assert!(missing.iter().any(|d| d.contains("[sv-SE]")), "{missing:?}");
    assert_eq!(from_doctor, missing, "doctor --against-db reports the same");

    for advice in &report {
        if advice.locale != "fr-FR" {
            client.batch_execute(&advice.index.as_ref().unwrap().ddl).await.unwrap();
        }
    }
    let checks = localized_index_checks(&url, &tls, &schema).await;
    client.batch_execute(&format!("DROP TABLE {TABLE}")).await.unwrap();
    assert!(checks.iter().all(|c| c.status == CheckStatus::Pass), "{checks:?}");
}

/// The localized schema the `doctor` index check is run against, over `table`.
fn localized_doctor_schema(table: &str) -> fraiseql_core::schema::CompiledSchema {
    fraiseql_core::schema::CompiledSchema::from_json(
        &format!(
            r#"{{"types": [{{"name": "Product", "sql_source": "{table}", "fields": [
                {{"name": "id", "field_type": "ID", "nullable": false}},
                {{"name": "name", "field_type": "String", "nullable": true, "localized": true}}]}}],
              "queries": [], "mutations": [], "subscriptions": [],
              "locale": {{"default": "en-US", "allowed": ["en-US", "fr-FR", "sv-SE"]}}}}"#
        ),
        false,
    )
    .unwrap()
}

/// The doctor suite's localized schema loads with no database.
#[test]
fn the_document_loads_without_a_database() {
    assert!(localized_doctor_schema("tv_product").locale.is_some());
    assert_eq!(header_doctor_schema().session_variables.variables.len(), 2);
}

/// #1520: `doctor --against-db` warns for each RLS policy that reads a `source = "header"`
/// session variable (a client-controlled value), and not for one that reads a `jwt` one.
#[tokio::test]
async fn doctor_warns_when_a_policy_reads_a_header_variable() {
    const TABLE: &str = "tb_doctor_header_policy";
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1520 doctor test: no DATABASE_URL");
        return;
    };
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {TABLE}; \
             CREATE TABLE {TABLE} (region text, tenant text); \
             ALTER TABLE {TABLE} ENABLE ROW LEVEL SECURITY; \
             CREATE POLICY by_region ON {TABLE} USING (region = current_setting('App.Region', true)); \
             CREATE POLICY by_tenant ON {TABLE} FOR INSERT WITH CHECK \
               (tenant = current_setting('app.tenant_id', true));"
        ))
        .await
        .unwrap();
    let schema = header_doctor_schema();
    let tls = fraiseql_db::postgres::PostgresTlsConfig::default();
    let mut file = Builder::new().suffix(".json").tempfile().unwrap();
    file.write_all(serde_json::to_string(&schema).unwrap().as_bytes()).unwrap();
    file.flush().unwrap();
    let checks: Vec<(CheckStatus, String)> =
        fraiseql_cli::commands::doctor::against_db_checks(&url, &tls, file.path(), &[])
            .await
            .into_iter()
            .filter(|c| c.name == "Header variable in RLS")
            .map(|c| (c.status, c.detail))
            .collect();
    client.batch_execute(&format!("DROP TABLE {TABLE}")).await.unwrap();

    let warned: Vec<&String> =
        checks.iter().filter(|(s, _)| *s == CheckStatus::Warn).map(|(_, d)| d).collect();
    assert!(
        warned
            .iter()
            .any(|d| d.contains(&format!("by_region on public.{TABLE} reads app.region"))),
        "the policy reading the header variable is named: {checks:?}"
    );
    assert!(
        !warned.iter().any(|d| d.contains("by_tenant")),
        "a policy reading a jwt variable is not: {checks:?}"
    );
}

/// `app.region` from the `x-region` header; `app.tenant_id` from the `tenant_id` claim.
fn header_doctor_schema() -> fraiseql_core::schema::CompiledSchema {
    fraiseql_core::schema::CompiledSchema::from_json(
        r#"{"types": [], "queries": [], "mutations": [], "subscriptions": [],
            "session_variables": {"variables": [
              {"name": "app.region", "source": "header", "header": "x-region"},
              {"name": "app.tenant_id", "source": "jwt", "claim": "tenant_id"}]}}"#,
        false,
    )
    .unwrap()
}

/// #1459: `doctor --against-db` names a fact table whose carried-forward measure no index
/// serves, with the `(entity…, over)` index as the remedy. Applying the remedy clears it; a
/// measure reduced within the bucket needs no seek and is not reported.
#[tokio::test]
async fn doctor_names_a_carried_forward_measure_no_index_serves() {
    use fraiseql_cli::commands::doctor::semi_additive_index_checks;
    use fraiseql_core::compiler::fact_table::{
        Additivity, DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn,
        SemiAdditiveReduction, SqlType,
    };

    const TABLE: &str = "tf_doctor_account_day";
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1459 doctor test: no DATABASE_URL");
        return;
    };
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {TABLE}; CREATE TABLE {TABLE} (account_id bigint, day date, \
             closing_balance bigint, average_balance bigint, data jsonb); \
             CREATE INDEX ON {TABLE} (day, account_id);"
        ))
        .await
        .unwrap();

    let semi = |using| Additivity::SemiAdditive {
        over: "day".to_string(),
        using,
        entity: vec!["account_id".to_string()],
    };
    let measure = |name: &str, additivity| MeasureColumn {
        name: name.to_string(),
        sql_type: SqlType::BigInt,
        nullable: false,
        additivity,
    };
    let filter = |name: &str, sql_type| FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: true,
        hierarchy: None,
    };
    let mut schema = fraiseql_core::schema::CompiledSchema::new();
    schema.add_fact_table(
        TABLE.to_string(),
        FactTableMetadata {
            table_name:               TABLE.to_string(),
            type_name:                None,
            measures:                 vec![
                measure("closing_balance", semi(SemiAdditiveReduction::Last)),
                measure("average_balance", semi(SemiAdditiveReduction::Avg)),
            ],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![
                filter("account_id", SqlType::BigInt),
                filter("day", SqlType::Date),
            ],
            calendar_dimensions:      vec![],
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        },
    );
    let tls = fraiseql_db::postgres::PostgresTlsConfig::default();

    // `(day, account_id)` leads with the wrong key: the seek is per account.
    let warned: Vec<_> = semi_additive_index_checks(&url, &tls, &schema)
        .await
        .into_iter()
        .filter(|c| c.status == CheckStatus::Warn)
        .collect();
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(warned[0].detail.contains("closing_balance"), "{warned:?}");
    assert!(
        !warned[0].detail.contains("average_balance"),
        "no seek within a bucket: {warned:?}"
    );

    // `doctor --against-db` runs the check, from the compiled schema file.
    let mut file = Builder::new().suffix(".json").tempfile().unwrap();
    file.write_all(serde_json::to_string(&schema).unwrap().as_bytes()).unwrap();
    file.flush().unwrap();
    let from_doctor: Vec<String> =
        fraiseql_cli::commands::doctor::against_db_checks(&url, &tls, file.path(), &[])
            .await
            .into_iter()
            .filter(|c| c.name == "Semi-additive index" && c.status == CheckStatus::Warn)
            .map(|c| c.detail)
            .collect();
    assert_eq!(from_doctor, vec![warned[0].detail.clone()], "doctor --against-db reports it");

    client.batch_execute(warned[0].hint.as_deref().unwrap()).await.unwrap();
    let after = semi_additive_index_checks(&url, &tls, &schema).await;
    assert!(after.iter().all(|c| c.status == CheckStatus::Pass), "{after:?}");
}
