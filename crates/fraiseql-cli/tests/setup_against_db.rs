//! Live-`PostgreSQL` integration test for `fraiseql setup` (#426).
//!
//! The embedded helper library (`sql/helpers/mutation_response.sql`) defines
//! dollar-quoted PL/pgSQL function bodies. The previous installer split the file
//! on `;` and executed fragments individually, which shredded those `$$…$$`
//! bodies and the trailing `DO`-block self-tests — so `fraiseql setup` failed on
//! a clean database and installed zero helpers. This test runs the real binary
//! against a database and asserts the helpers install and are callable.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free
//! test leg (even under `--all-features`).
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL`
//! **Parallelism:** installs into the shared `fraiseql` schema via idempotent
//!   `CREATE OR REPLACE`; safe to repeat.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::process::Command;

use tokio_postgres::NoTls;

#[tokio::test]
async fn setup_installs_dollar_quoted_helpers() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #426 setup against-db test: DATABASE_URL not set");
        return;
    };

    // Run the real installer. Before the fix this exits non-zero because the
    // `split(';')` loop produces broken SQL fragments on the first `$$` body.
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["setup", "--database", &url])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fraiseql setup must install dollar-quoted helpers and pass the file's \
         own DO-block self-tests; exit={:?}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify the three helpers exist and are callable.
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let version: String = client
        .query_one("SELECT fraiseql.library_version() AS v", &[])
        .await
        .unwrap()
        .get("v");
    assert_eq!(version, "2.3.0", "library_version() must report the installed version");

    // mutation_ok / mutation_err return the 13-column response and are callable.
    let ok_succeeded: bool = client
        .query_one("SELECT succeeded FROM fraiseql.mutation_ok('{\"id\":\"x\"}'::jsonb)", &[])
        .await
        .unwrap()
        .get("succeeded");
    assert!(ok_succeeded, "mutation_ok must return succeeded=true");

    let err_succeeded: bool = client
        .query_one("SELECT succeeded FROM fraiseql.mutation_err('not_found')", &[])
        .await
        .unwrap()
        .get("succeeded");
    assert!(!err_succeeded, "mutation_err must return succeeded=false");
}

/// #569: `fraiseql setup` must also install `core.tb_entity_change_log` — the table every
/// default mutation's transactional-outbox CTE writes. Without it, the first mutation on a
/// freshly authored stack fails at prepare with a bare
/// `relation "core.tb_entity_change_log" does not exist`. The contract DDL is idempotent,
/// so running setup here (which the sibling test also does) is safe to repeat.
#[tokio::test]
async fn setup_installs_change_log_contract() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #569 setup change-log against-db test: DATABASE_URL not set");
        return;
    };

    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["setup", "--database", &url])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fraiseql setup must install the change-log contract; exit={:?}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });

    // The contract table exists after setup.
    let present: bool = client
        .query_one("SELECT to_regclass('core.tb_entity_change_log') IS NOT NULL AS present", &[])
        .await
        .unwrap()
        .get("present");
    assert!(present, "setup must install core.tb_entity_change_log (#569)");

    // The NOT-NULL backbone columns the outbox CTE relies on are present.
    let backbone: i64 = client
        .query_one(
            "SELECT count(*) AS n FROM information_schema.columns \
             WHERE table_schema = 'core' AND table_name = 'tb_entity_change_log' \
               AND column_name IN ('object_type', 'modification_type')",
            &[],
        )
        .await
        .unwrap()
        .get("n");
    assert_eq!(backbone, 2, "the change-log contract backbone columns must be present");
}

/// Ruling AG 1: `fraiseql.mutation_err` can say which declared error it produced. The stamp
/// is its last parameter, so every call the four-argument helper accepted binds as before —
/// and none becomes ambiguous against a second overload.
#[tokio::test]
async fn mutation_err_stamps_the_error_type_it_produced() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping AG 1 setup against-db test: DATABASE_URL not set");
        return;
    };
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["setup", "--database", &url])
        .output()
        .unwrap();
    assert!(out.status.success(), "setup: {}", String::from_utf8_lossy(&out.stderr));
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let stamped: Option<String> = client
        .query_one(
            "SELECT entity_type FROM fraiseql.mutation_err('conflict', 'duplicate', \
             p_entity_type => 'DuplicateEmailError')",
            &[],
        )
        .await
        .unwrap()
        .get("entity_type");
    assert_eq!(stamped.as_deref(), Some("DuplicateEmailError"));

    for call in [
        "fraiseql.mutation_err('not_found')",
        "fraiseql.mutation_err('validation', 'bad')",
        "fraiseql.mutation_err('validation', 'bad', '{\"field\": \"email\"}'::jsonb)",
        "fraiseql.mutation_err('validation', 'bad', NULL, 422::smallint)",
    ] {
        let row = client.query_one(&format!("SELECT entity_type FROM {call}"), &[]).await;
        assert!(row.is_ok(), "{call} must still bind: {row:?}");
        let row = row.unwrap();
        let entity_type: Option<String> = row.get("entity_type");
        assert!(entity_type.is_none(), "{call} stamps nothing");
    }
}

/// The 2.2.0 `fraiseql.mutation_err`, verbatim: four arguments, no stamp.
const MUTATION_ERR_2_2_0: &str = r"
CREATE SCHEMA IF NOT EXISTS fraiseql;
DROP FUNCTION IF EXISTS fraiseql.mutation_err(TEXT, TEXT, JSONB, SMALLINT, TEXT);
CREATE OR REPLACE FUNCTION fraiseql.mutation_err(
    p_error_class TEXT,
    p_message TEXT DEFAULT '',
    p_error_detail JSONB DEFAULT NULL,
    p_http_status SMALLINT DEFAULT NULL
)
RETURNS TABLE(
    succeeded BOOLEAN, state_changed BOOLEAN, error_class TEXT, status_detail TEXT,
    http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB,
    updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB
) AS $$
BEGIN
    RETURN QUERY SELECT FALSE, FALSE, p_error_class, NULL::TEXT, p_http_status,
        COALESCE(p_message, ''), NULL::UUID, NULL::TEXT, NULL::JSONB, NULL::TEXT[],
        NULL::JSONB, p_error_detail, NULL::JSONB;
END;
$$ LANGUAGE plpgsql IMMUTABLE;
";

/// Ruling AG 1: a database still on the 2.2.0 helpers upgrades in place. Its four-argument
/// `mutation_err` is replaced, not overloaded — a five-argument overload beside it would
/// make every call that omits the stamp ambiguous ("function … is not unique").
///
/// Runs the SQL `fraiseql setup` embeds, over a 2.2.0 install, inside one transaction that
/// is rolled back: the shared `fraiseql` schema the sibling tests use never sees the old
/// signature.
#[tokio::test]
async fn the_helpers_replace_a_2_2_0_mutation_err() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping AG 1 upgrade test: DATABASE_URL not set");
        return;
    };
    let (mut client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let tx = client.transaction().await.unwrap();
    tx.batch_execute(MUTATION_ERR_2_2_0).await.unwrap();
    tx.batch_execute(include_str!("../sql/helpers/mutation_response.sql"))
        .await
        .unwrap();

    for call in [
        "fraiseql.mutation_err('not_found')",
        "fraiseql.mutation_err('validation', 'bad')",
        "fraiseql.mutation_err('validation', 'bad', NULL, 422::smallint)",
        "fraiseql.mutation_err('conflict', 'dup', p_entity_type => 'DuplicateEmailError')",
    ] {
        let row = tx.query_one(&format!("SELECT entity_type FROM {call}"), &[]).await;
        assert!(row.is_ok(), "{call} must bind after the upgrade: {row:?}");
    }
    let overloads: i64 = tx
        .query_one(
            "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = 'fraiseql' AND p.proname = 'mutation_err'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(overloads, 1, "the 2.2.0 signature must be replaced, not overloaded");
    tx.rollback().await.unwrap();
}
