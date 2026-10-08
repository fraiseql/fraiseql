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

/// Connect and open a transaction over the helper SQL `fraiseql setup` embeds; the caller
/// rolls it back, so the shared `fraiseql` schema never sees an unreleased helper.
async fn helpers_in_rollback(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute("BEGIN").await.unwrap();
    client
        .batch_execute(include_str!("../sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    client
}

/// #1425: `fraiseql.error_entry` turns the identifier a mutation function passes into the
/// key a client translates (`t('errors.' + identifier)`): unaccented, camelCase split,
/// every run of other characters one `_`, matching `^[a-z][a-z0-9_]*$`. The corpus is the
/// issue's: identifiers built from a human label or a type name.
#[tokio::test]
async fn error_entry_normalises_the_identifier_into_a_translation_key() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1425 error_entry test: DATABASE_URL not set");
        return;
    };
    let client = helpers_in_rollback(&url).await;

    for (identifier, key) in [
        ("order line_not_found", "order_line_not_found"),
        ("Order line_not_found", "order_line_not_found"),
        ("PaymentTerm", "payment_term"),
        ("paymentTerm_not_found", "payment_term_not_found"),
        ("Événement", "evenement"),
        ("  déjà  vu!! ", "deja_vu"),
        ("HTTPServer_error", "http_server_error"),
        ("already_snake", "already_snake"),
        ("Straße", "strasse"),
    ] {
        let row = client
            .query_one(
                "SELECT fraiseql.error_entry(1::smallint, $1, 'msg') AS e",
                &[&identifier],
            )
            .await;
        assert!(row.is_ok(), "error_entry({identifier:?}) failed: {row:?}");
        let entry: serde_json::Value = row.unwrap().get("e");
        assert_eq!(entry["identifier"], key, "error_entry({identifier:?})");
    }

    let entry: serde_json::Value = client
        .query_one(
            "SELECT fraiseql.error_entry(404::smallint, 'NotFound', 'No such order', \
             '{\"id\": 7}'::jsonb) AS e",
            &[],
        )
        .await
        .unwrap()
        .get("e");
    assert_eq!(
        entry,
        serde_json::json!({
            "code": 404, "identifier": "not_found", "message": "No such order",
            "details": {"id": 7}
        }),
        "an entry carries code, identifier, message and details"
    );

    for nothing in ["--", "", "   ", "404"] {
        let refused = client
            .query_one("SELECT fraiseql.error_entry(1::smallint, $1, 'msg')", &[&nothing])
            .await;
        assert!(refused.is_err(), "{nothing:?} normalises to no valid key and must raise");
    }
    client.batch_execute("ROLLBACK").await.unwrap();
}

/// #1425: `fraiseql.mutation_err_entries` is `mutation_err` whose `error_detail` is the
/// `{"errors": [...]}` the given entries make.
#[tokio::test]
async fn mutation_err_entries_wraps_its_entries_as_errors() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1425 mutation_err_entries test: DATABASE_URL not set");
        return;
    };
    let client = helpers_in_rollback(&url).await;

    let row = client
        .query_one(
            "SELECT succeeded, error_class, message, error_detail FROM \
             fraiseql.mutation_err_entries('validation', 'Invalid order', \
               fraiseql.error_entry(422::smallint, 'Order line_not_found', 'No line'), \
               fraiseql.error_entry(422::smallint, 'quantityTooLow', 'Too low'))",
            &[],
        )
        .await
        .unwrap();
    let succeeded: bool = row.get("succeeded");
    let class: String = row.get("error_class");
    let detail: serde_json::Value = row.get("error_detail");
    assert!(!succeeded, "an error response");
    assert_eq!(class, "validation");
    assert_eq!(
        detail,
        serde_json::json!({"errors": [
            {"code": 422, "identifier": "order_line_not_found", "message": "No line"},
            {"code": 422, "identifier": "quantity_too_low", "message": "Too low"},
        ]})
    );
    client.batch_execute("ROLLBACK").await.unwrap();
}
