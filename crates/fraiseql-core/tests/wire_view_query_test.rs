#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr
)] // Reason: test code, panics are acceptable

//! Test querying views with fraiseql-wire

#[cfg(all(
    feature = "postgres",
    feature = "wire-backend",
    feature = "test-postgres"
))]
mod wire_view_tests {
    use fraiseql_core::db::{DatabaseAdapter, FraiseWireAdapter, postgres::PostgresAdapter};

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
    const VIEW: &str = "v_wire_view_query_user";
    const TABLE: &str = "tb_wire_view_query_user";

    /// A real view over a real table, not a table wearing a `v_` name: this suite's
    /// subject is querying a *view* through the wire protocol.
    ///
    /// The names carry this fixture's own prefix, and the row count differs from
    /// every neighbouring fixture, so a test reading the wrong relation fails
    /// rather than passing on a plausible-looking row.
    const SEED: &str = r#"
      ('{"id":"wire-view-1","name":"Wire View Fiona","email":"fiona@wire-view.example"}'::jsonb),
      ('{"id":"wire-view-2","name":"Wire View Gus","email":"gus@wire-view.example"}'::jsonb),
      ('{"id":"wire-view-3","name":"Wire View Hana","email":"hana@wire-view.example"}'::jsonb)
    "#;

    /// The number of rows [`SEED`] inserts.
    const SEEDED_ROWS: usize = 3;

    /// The prefix every seeded `name` carries.
    const NAME_PREFIX: &str = "Wire View ";

    /// Creates the fixture and returns the connection string the adapter should use.
    ///
    /// The DDL cannot go through `FraiseWireAdapter`: its `execute_raw_query`
    /// always returns an error by design — the wire protocol path supports only
    /// `SELECT data FROM v_*` shapes — so it goes through `PostgresAdapter`, as
    /// `selection_conformance_postgres.rs` does for the same reason.
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
                .unwrap_or_else(|e| panic!("provision the wire_view_query fixture ({stmt}): {e}"));
        }

        conn_str
    }

    #[tokio::test]
    async fn test_query_v_users_view() {
        let conn_str = provision().await;

        println!("Connecting to: {}", conn_str);

        let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);

        println!("Querying {VIEW} with limit 10...");

        // A limit above the fixture, so a view returning more than it seeds shows
        // up in the count instead of being clipped to it.
        let results = adapter.execute_where_query(VIEW, None, Some(10), None, None).await;

        match &results {
            Ok(rows) => {
                println!("SUCCESS: Got {} rows", rows.len());
                if let Some(first) = rows.first() {
                    println!("First row: {:?}", first);
                }
            },
            Err(e) => {
                println!("ERROR: {}", e);
            },
        }

        let rows = results.expect("query the view this test provisioned");
        assert_eq!(rows.len(), SEEDED_ROWS, "should get the {SEEDED_ROWS} rows this test seeded");

        // Order-independent: the view has no ORDER BY, so identity is asserted per
        // row rather than by position.
        for row in &rows {
            let name =
                row.data.get("name").and_then(serde_json::Value::as_str).unwrap_or_else(|| {
                    panic!("every seeded row carries a string `name`, got {:?}", row.data)
                });
            assert!(
                name.starts_with(NAME_PREFIX),
                "row came from this test's fixture, not a shared one: {name:?}"
            );
        }
    }
}
