//! Live-PostgreSQL integration test for the `fraiseql sources` cursor reader (#573).
//!
//! It installs the real `_fraiseql_source_cursor` table from the shipped observers
//! migration, seeds a known advanced watermark, and asserts
//! [`SourceCursorReader::load_cursors`] decodes every column — the opaque JSONB
//! value, the compare-and-swap version, the `updated_at` text, and the DB-clock lag.
//!
//! Self-skips when no `DATABASE_URL` is set. Like every CLI `*_against_db` test this
//! is **local-only** verification — no Dagger leg runs the CLI against a live DB. Run
//! against the warm dev database:
//!
//! ```bash
//! DATABASE_URL=postgres://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-cli --features test-postgres --test sources_against_db
//! ```

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::commands::sources::reader::{CursorRow, SourceCursorReader};
use fraiseql_observers::migrations::source_cursor_sql;

/// A source name unique to this test so leftover rows in the shared throwaway DB
/// (e.g. from the server poller tests) never collide with the assertions.
const SEEDED: &str = "against-db-orders";
/// A second seeded source with a NULL cursor value, to prove the reader tolerates it.
const SEEDED_NULL: &str = "against-db-null-value";

/// Install the cursor table from the shipped migration and seed known rows.
async fn provision(url: &str) -> Option<tokio_postgres::Client> {
    let (client, conn) = match tokio_postgres::connect(url, tokio_postgres::NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #573 sources reader test: {e}");
            return None;
        },
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Idempotent DDL from the shipped migration (matches the runtime store's init).
    client.batch_execute(source_cursor_sql()).await.unwrap();
    client
        .batch_execute(&format!(
            "INSERT INTO _fraiseql_source_cursor (source_name, cursor_value, version, updated_at) \
             VALUES ('{SEEDED}', '{{\"page\": 4}}'::jsonb, 4, now() - interval '30 seconds'), \
                    ('{SEEDED_NULL}', NULL, 2, now()) \
             ON CONFLICT (source_name) DO UPDATE \
               SET cursor_value = EXCLUDED.cursor_value, version = EXCLUDED.version, \
                   updated_at = EXCLUDED.updated_at;"
        ))
        .await
        .unwrap();

    Some(client)
}

fn find<'a>(rows: &'a [CursorRow], source: &str) -> &'a CursorRow {
    rows.iter().find(|r| r.source_name == source).expect("seeded source present")
}

#[tokio::test]
async fn reader_decodes_the_advanced_watermark_and_lag() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    if provision(&url).await.is_none() {
        return;
    }

    let reader = SourceCursorReader::connect(&url).expect("connect source cursor reader");
    let rows = reader.load_cursors().await.expect("load cursors");

    // The advanced row: opaque JSONB value + version + a positive DB-clock lag.
    let orders = find(&rows, SEEDED);
    assert_eq!(orders.version, 4);
    assert_eq!(orders.value, Some(serde_json::json!({ "page": 4 })));
    assert!(
        orders.age_seconds >= 25.0,
        "≈30s of lag from the DB clock: {}",
        orders.age_seconds
    );
    assert!(!orders.updated_at.is_empty(), "updated_at decodes to text");

    // A row with a SQL NULL cursor value decodes to `None`, not an error.
    let null_value = find(&rows, SEEDED_NULL);
    assert_eq!(null_value.version, 2);
    assert_eq!(null_value.value, None);
}

#[tokio::test]
async fn non_postgres_url_is_rejected() {
    assert!(
        SourceCursorReader::connect("mysql://localhost/db").is_err(),
        "non-postgres URL rejected"
    );
}
