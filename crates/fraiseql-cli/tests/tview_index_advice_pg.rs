//! #1571: `compile --database` advises the `pg_tviews` `indexes` option for the document keys
//! a query over a TVIEW filters on every call, and the statement it prints declares an index
//! FraiseQL's own reads are served by.
//!
//! `tv_member` keeps its rows' `email`, `age` and `active` only in its document, so
//! `memberByEmail(email:)`, `membersAged(age:)` and `activeMembers(active:)` compare
//! `data->>'email'`, `(data->>'age')::numeric` and `(data->>'active')::boolean` on every call,
//! and nothing indexes them. The TVIEW is declared with a non-default `fillfactor`: a replace's
//! options are its whole declaration, so the advice must carry it, or applying it would reset
//! it (and, the other way round, a replace without `indexes` drops them).
//!
//! Needs `pg_tviews`: the `postgres-tviews-test` service (`docker/pg-tviews`), through
//! `TVIEWS_DATABASE_URL`. Without it the suite skips.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:** `TVIEWS_DATABASE_URL`
//! · **Parallelism:** drops and recreates its own `p1571x` schema → run `--test-threads=1`.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc, time::Duration};

use fraiseql_cli::schema::{
    ConvertOptions, SchemaConverter,
    database_validator::{DatabaseWarning, validate_schema_against_database},
    intermediate::IntermediateSchema,
};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresIntrospector},
    runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_db::postgres::PostgresTlsConfig;
use serde_json::{Value, json};

const SCHEMA: &str = "p1571x";
const MEMBERS: usize = 2_000;

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

async fn provision(client: &tokio_postgres::Client) {
    client
        .batch_execute(&format!(
            "CREATE EXTENSION IF NOT EXISTS pg_tviews;
             DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{SCHEMA}' LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tb_member (pk_member bigint GENERATED ALWAYS AS IDENTITY
               PRIMARY KEY, id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
               email text NOT NULL, age int NOT NULL, active boolean NOT NULL);
             INSERT INTO {SCHEMA}.tb_member (email, age, active)
               SELECT 'm' || n || '@example.com', n % 90, n % 50 = 0
               FROM generate_series(1, {MEMBERS}) n;
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_member', $T$
               SELECT m.pk_member, m.id, jsonb_build_object('id', m.id, 'email', m.email,
                 'age', m.age, 'active', m.active) AS data
               FROM {SCHEMA}.tb_member m $T$, '{{\"typename\": \"Member\", \"fillfactor\": 70}}');
             ANALYZE {SCHEMA}.tv_member;"
        ))
        .await
        .unwrap();
}

/// Three queries, each filtering the document on one key by its argument.
fn schema() -> CompiledSchema {
    let field = |name: &str, ty: &str| json!({ "name": name, "type": ty, "nullable": false });
    let query = |name: &str, list: bool, arg: (&str, &str)| {
        json!({ "name": name, "return_type": "Member", "returns_list": list, "nullable": !list,
                "sql_source": format!("{SCHEMA}.tv_member"), "arguments": [field(arg.0, arg.1)] })
    };
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{ "name": "Member", "sql_source": format!("{SCHEMA}.tv_member"),
                    "is_input": false,
                    "fields": [field("id", "ID"), field("email", "String"), field("age", "Int"),
                               field("active", "Boolean")] }],
        "queries": [
            query("memberByEmail", false, ("email", "String")),
            query("membersAged", true, ("age", "Int")),
            query("activeMembers", true, ("active", "Boolean")),
        ],
        "mutations": []
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.build_indexes();
    schema
}

/// What `compile --database` advises: each query's index warning, by query.
async fn advice(url: &str, schema: &CompiledSchema) -> HashMap<String, DatabaseWarning> {
    let pool = fraiseql_cli::connection::postgres_pool(url, "test", &PostgresTlsConfig::default())
        .await
        .unwrap();
    let introspector = PostgresIntrospector::new(pool);
    validate_schema_against_database(schema, &introspector)
        .await
        .unwrap()
        .warnings
        .into_iter()
        .filter_map(|w| {
            let DatabaseWarning::TviewIndexMissing { query_name, .. } = &w else {
                return None;
            };
            Some((query_name.clone(), w))
        })
        .collect()
}

/// Index scans on `tv_member`'s indexes on `data`, by index.
async fn scans(client: &tokio_postgres::Client) -> HashMap<String, i64> {
    client
        .batch_execute("SELECT pg_stat_force_next_flush(); SELECT pg_stat_clear_snapshot();")
        .await
        .unwrap();
    client
        .query(
            "SELECT indexrelname::text, idx_scan FROM pg_stat_user_indexes \
             WHERE schemaname = $1 AND relname = 'tv_member' AND indexrelname LIKE '%data%'",
            &[&SCHEMA],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0), r.get(1)))
        .collect()
}

#[tokio::test]
async fn the_indexes_advice_declares_an_index_fraiseql_reads_are_served_by() {
    let Some(url) = tviews_database_url() else {
        eprintln!("skipping #1571: TVIEWS_DATABASE_URL not set");
        return;
    };
    let admin = client(&url).await;
    provision(&admin).await;
    let schema = schema();

    let advised = advice(&url, &schema).await;
    let keys = |query: &str| match &advised[query] {
        DatabaseWarning::TviewIndexMissing { keys, .. } => keys.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(keys("memberByEmail"), ["email"]);
    assert_eq!(keys("membersAged"), ["age"]);
    assert_eq!(keys("activeMembers"), ["active"]);

    // Apply each statement as printed, one after the other: each carries the TVIEW's options.
    for query in ["memberByEmail", "membersAged", "activeMembers"] {
        let statement = advised[query].statement().unwrap();
        admin
            .batch_execute(&statement)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
        let fresh = advice(&url, &schema).await;
        assert!(!fresh.contains_key(query), "{query} is served now: {fresh:?}");
    }
    let options: Value = admin
        .query_one(
            "SELECT options FROM tviews.registry WHERE schema = $1 AND name = 'tv_member'",
            &[&SCHEMA],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        options["fillfactor"],
        json!(70),
        "the replace kept the other options: {options}"
    );
    assert_eq!(options["typename"], json!("Member"), "{options}");
    assert!(advice(&url, &schema).await.is_empty(), "nothing left to advise");
    admin.batch_execute(&format!("ANALYZE {SCHEMA}.tv_member;")).await.unwrap();

    // FraiseQL's reads are served by those indexes: each moves its own scan counter.
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let executor = Executor::new(schema.clone(), adapter);
    for (query, document) in [
        ("email", "{ memberByEmail(email: \"m7@example.com\") { id } }"),
        ("age", "{ membersAged(age: 7) { id } }"),
        ("active", "{ activeMembers(active: true) { id } }"),
    ] {
        let before = scans(&admin).await;
        let body = executor.execute(document, None).await.unwrap();
        assert!(!body.to_string().contains("\"errors\""), "{document}: {body}");
        // A backend flushes its statistics at most once a second, and an idle one within ten.
        let mut moved = Vec::new();
        for _ in 0..150 {
            let after = scans(&admin).await;
            moved = after
                .iter()
                .filter(|(name, n)| **n > before.get(*name).copied().unwrap_or(0))
                .map(|(name, _)| name.clone())
                .collect();
            if !moved.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            moved.len() == 1 && moved[0].contains(query),
            "{document} is served by the index on `{query}`, and only it: {moved:?}"
        );
    }
}
