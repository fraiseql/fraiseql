#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr
)] // Reason: test code, panics acceptable
//! Test fraiseql-wire directly without adapter layer
//!
//! Run with: cargo test -p fraiseql-core --features
//! postgres,wire-backend,test-postgres --test `wire_direct_test`

#[cfg(all(
    feature = "postgres",
    feature = "wire-backend",
    feature = "test-postgres"
))]
mod wire_direct_tests {
    use fraiseql_core::db::{postgres::PostgresAdapter, traits::DatabaseAdapter};
    use fraiseql_wire::FraiseClient;
    use futures::StreamExt;

    /// Uniquely named so it cannot collide with the fixtures other suites share in
    /// the same database.
    ///
    /// This test used to query the `v_user` that `tests/sql/postgres/init.sql`
    /// seeds into `public` at container init, and provisioned nothing itself. Any
    /// suite that redefines `public.tb_user` takes `public.v_user` with it — the
    /// measured case is a database seeded by `docker/e2e/init-postgres.sql`, whose
    /// `CREATE TABLE IF NOT EXISTS tb_user (id SERIAL, name TEXT)` silently wins
    /// over init.sql's `IF NOT EXISTS` and leaves `v_user` uncreatable — and this
    /// then failed with `relation "v_user" does not exist (42P01)`, a failure that
    /// reads as a regression in whatever change is being tested (#1229).
    const VIEW: &str = "v_wire_direct_user";
    const TABLE: &str = "tb_wire_direct_user";

    /// Distinct from the row count and the names any other fixture seeds, so a
    /// test reading the wrong relation is visible rather than plausible.
    const SEED: &str = r#"
      ('{"id":"wire-direct-1","name":"Wire Direct Alice","email":"alice@wire-direct.example"}'::jsonb),
      ('{"id":"wire-direct-2","name":"Wire Direct Bob","email":"bob@wire-direct.example"}'::jsonb),
      ('{"id":"wire-direct-3","name":"Wire Direct Carol","email":"carol@wire-direct.example"}'::jsonb),
      ('{"id":"wire-direct-4","name":"Wire Direct Dave","email":"dave@wire-direct.example"}'::jsonb),
      ('{"id":"wire-direct-5","name":"Wire Direct Erin","email":"erin@wire-direct.example"}'::jsonb)
    "#;

    /// The number of rows [`SEED`] inserts. The assertion below is about this
    /// fixture's contents, not about whatever the shared seed happens to hold.
    const SEEDED_ROWS: usize = 5;

    /// Creates the fixture and returns the connection string the client should use.
    ///
    /// fraiseql-wire cannot do this itself: `FraiseWireAdapter::execute_raw_query`
    /// always returns an error by design — the wire protocol path supports only
    /// `SELECT data FROM v_*` shapes — so the DDL goes through `PostgresAdapter`,
    /// as `selection_conformance_postgres.rs` does for the same reason.
    async fn provision() -> String {
        let conn_str = fraiseql_test_support::database_url();
        let admin = PostgresAdapter::new(&conn_str).await.expect("connect to the bound PostgreSQL");

        for stmt in [
            format!("DROP VIEW IF EXISTS {VIEW}"),
            format!("DROP TABLE IF EXISTS {TABLE}"),
            format!(
                "CREATE TABLE {TABLE} (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), \
                 data jsonb NOT NULL)"
            ),
            format!("INSERT INTO {TABLE} (data) VALUES {SEED}"),
            format!("CREATE VIEW {VIEW} AS SELECT id, data FROM {TABLE}"),
        ] {
            admin
                .execute_raw_query(&stmt)
                .await
                .unwrap_or_else(|e| panic!("provision the wire_direct fixture ({stmt}): {e}"));
        }

        conn_str
    }

    #[tokio::test]
    async fn test_direct_v_user_query() {
        let conn_str = provision().await;

        println!("Connecting to: {}", conn_str);

        let client = FraiseClient::connect(&conn_str).await.unwrap();

        println!("Querying {VIEW} directly...");

        let stream_result =
            client.query::<serde_json::Value>(VIEW).chunk_size(1024).execute().await;

        match &stream_result {
            Ok(_) => println!("Query executed successfully"),
            Err(e) => println!("Query failed with error: {:?}", e),
        }

        let mut stream = stream_result.unwrap();

        let mut count = 0;
        while let Some(result) = stream.next().await {
            match result {
                Ok(_item) => {
                    count += 1;
                    // One past the fixture, so a view returning more than it seeds
                    // is caught by the assertion rather than truncated away.
                    if count > SEEDED_ROWS {
                        break;
                    }
                },
                Err(e) => {
                    println!("ERROR: {}", e);
                    panic!("Query failed: {}", e);
                },
            }
        }

        println!("SUCCESS: Got {} rows", count);
        assert_eq!(count, SEEDED_ROWS, "should get the {SEEDED_ROWS} rows this test seeded");
    }
}
