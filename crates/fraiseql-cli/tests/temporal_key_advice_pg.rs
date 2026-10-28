//! `compile --database` advises every relation whose query sorts or filters a date it reads out
//! of the document (#1815), and never offers a TVIEW's `pg_tviews` row timestamps as that date
//! (#1818).
//!
//! FraiseQL compares such a key as `(data->>'key')::timestamptz`, a cast PostgreSQL will not
//! index, on a view or a table as much as on a TVIEW. What fixes it depends on the relation,
//! and a TVIEW is recognised from `pg_tviews`' catalog, never from its `tv_` name:
//!
//! - without `pg_tviews` (`DATABASE_URL`), a `tv_` table is a table, and its `created_at` is its
//!   own;
//! - with `pg_tviews` (`TVIEWS_DATABASE_URL`, the `postgres-tviews-test` service), the `created_at`
//!   it appends to a TVIEW holds the time it wrote each row, so no row's equals the document's
//!   date, and the advice must not name it.
//!
//! Both cases are `#[ignore]`d, so a leg without these databases (the workspace test leg)
//! reports them as ignored rather than passed. The server leg binds both databases and runs
//! the binary with `--ignored`, and there a missing URL fails the case instead of skipping it.
//!
//! **Execution engine:** `PostgreSQL` (+ `pg_tviews`) · **Infrastructure:** `DATABASE_URL`,
//! `TVIEWS_DATABASE_URL` · **Parallelism:** drops and recreates its own `p1815` schema → run
//! `--test-threads=1`.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use std::collections::BTreeMap;

use fraiseql_cli::schema::{
    ConvertOptions, SchemaConverter,
    database_validator::{
        DatabaseWarning, TemporalKeyRemedy, create_introspector, validate_schema_against_database,
    },
    intermediate::IntermediateSchema,
};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_db::postgres::PostgresTlsConfig;
use serde_json::json;

const SCHEMA: &str = "p1815";

/// The URL in `var`. Set wherever this ignored suite is asked to run, so its absence fails.
fn url(var: &str) -> String {
    std::env::var(var).ok().filter(|u| !u.is_empty()).unwrap_or_else(|| {
        panic!("{var} is not set: this suite runs where both databases are bound (module docs)")
    })
}

async fn client(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
}

/// `tb_post`, dated in 2020, and two views over it: `v_post` keeps the dates only in its
/// document, `v_post_dated` also exposes the table's `created_at`.
fn base_ddl() -> String {
    format!(
        "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
         CREATE SCHEMA {SCHEMA};
         CREATE TABLE {SCHEMA}.tb_post (pk_post bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
           id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
           created_at timestamptz NOT NULL, published_at timestamptz NOT NULL);
         INSERT INTO {SCHEMA}.tb_post (created_at, published_at)
           SELECT '2020-01-01'::timestamptz + n * interval '1 hour',
                  '2020-06-01'::timestamptz + n * interval '1 hour'
           FROM generate_series(1, 50) n;
         CREATE VIEW {SCHEMA}.v_post AS
           SELECT id, jsonb_build_object('id', id, 'created_at', created_at,
             'published_at', published_at) AS data FROM {SCHEMA}.tb_post;
         CREATE VIEW {SCHEMA}.v_post_dated AS
           SELECT id, created_at, jsonb_build_object('id', id, 'created_at', created_at,
             'published_at', published_at) AS data FROM {SCHEMA}.tb_post;"
    )
}

/// One sorting list query per relation, each returning `Post { createdAt, publishedAt }`.
fn schema(relations: &[&str]) -> CompiledSchema {
    let field = |name: &str, ty: &str| json!({ "name": name, "type": ty, "nullable": false });
    let queries: Vec<_> = relations
        .iter()
        .map(|relation| {
            json!({ "name": relation, "return_type": "Post", "returns_list": true,
                    "nullable": false, "sql_source": format!("{SCHEMA}.{relation}"),
                    "arguments": [], "auto_params": { "order_by": true } })
        })
        .collect();
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{ "name": "Post", "sql_source": format!("{SCHEMA}.v_post"), "is_input": false,
                    "fields": [field("id", "ID"), field("createdAt", "DateTime"),
                               field("publishedAt", "DateTime")] }],
        "queries": queries,
        "mutations": []
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.build_indexes();
    schema
}

/// Each `(query, field)` `compile --database` advises on, with its remedy, and the messages.
async fn advice(
    url: &str,
    schema: &CompiledSchema,
) -> (BTreeMap<(String, String), TemporalKeyRemedy>, Vec<String>) {
    // The introspector `compile --database` itself runs through.
    let introspector = create_introspector(url, &PostgresTlsConfig::default()).await.unwrap();
    let warnings = validate_schema_against_database(schema, &introspector).await.unwrap().warnings;
    let mut remedies = BTreeMap::new();
    let mut messages = Vec::new();
    for warning in &warnings {
        if let DatabaseWarning::TemporalKeyInDocument {
            query_name,
            field,
            remedy,
            ..
        } = warning
        {
            remedies.insert((query_name.clone(), field.clone()), *remedy);
            messages.push(warning.to_string());
        }
    }
    (remedies, messages)
}

fn key(query: &str, field: &str) -> (String, String) {
    (query.to_string(), field.to_string())
}

#[tokio::test]
#[ignore = "needs PostgreSQL: the server leg runs it with --ignored"]
async fn without_pg_tviews_every_relation_is_advised_by_what_it_is() {
    let url = url("DATABASE_URL");
    let admin = client(&url).await;
    admin
        .batch_execute(&format!(
            "{}
             CREATE TABLE {SCHEMA}.tv_post AS SELECT id, created_at, data FROM {SCHEMA}.v_post_dated;",
            base_ddl()
        ))
        .await
        .unwrap();

    let (remedies, _) = advice(&url, &schema(&["v_post", "v_post_dated", "tv_post"])).await;

    assert_eq!(
        remedies,
        BTreeMap::from([
            (key("v_post", "createdAt"), TemporalKeyRemedy::ExposeFromView),
            (key("v_post", "publishedAt"), TemporalKeyRemedy::ExposeFromView),
            (key("v_post_dated", "createdAt"), TemporalKeyRemedy::MapColumn),
            (key("v_post_dated", "publishedAt"), TemporalKeyRemedy::ExposeFromView),
            // No pg_tviews: a `tv_` table is a table, and its `created_at` is its own.
            (key("tv_post", "createdAt"), TemporalKeyRemedy::MapColumn),
            (key("tv_post", "publishedAt"), TemporalKeyRemedy::AddToTable),
        ])
    );
}

#[tokio::test]
#[ignore = "needs PostgreSQL with pg_tviews: the server leg runs it with --ignored"]
async fn a_tviews_row_timestamps_are_never_offered_as_its_documents_date() {
    let url = url("TVIEWS_DATABASE_URL");
    let admin = client(&url).await;
    admin
        .batch_execute(&format!(
            "CREATE EXTENSION IF NOT EXISTS pg_tviews;
             DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{SCHEMA}' LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             {}
             -- A TVIEW's name is unique across schemas, and the leg's other suites
             -- share this database, so this one is named for this suite alone.
             SELECT tviews.pg_tviews_create('{SCHEMA}.tv_dated_post', $T$
               SELECT p.pk_post AS pk_dated_post, p.id,
                 jsonb_build_object('id', p.id, 'created_at', p.created_at,
                   'published_at', p.published_at) AS data
               FROM {SCHEMA}.tb_post p $T$, '{{\"typename\": \"DatedPost\"}}');
             CREATE TABLE {SCHEMA}.tv_plain (id uuid, data jsonb);
             INSERT INTO {SCHEMA}.tv_plain SELECT id, data FROM {SCHEMA}.v_post;",
            base_ddl()
        ))
        .await
        .unwrap();

    // The premise: pg_tviews' `created_at` holds when it wrote the row, not the post's date.
    let same: i64 = admin
        .query_one(
            &format!(
                "SELECT count(*) FROM {SCHEMA}.tv_dated_post \
                 WHERE created_at = (data->>'created_at')::timestamptz"
            ),
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(same, 0, "no row's pg_tviews created_at is its document's date");

    let (remedies, messages) =
        advice(&url, &schema(&["tv_dated_post", "tv_plain", "v_post"])).await;

    assert_eq!(
        remedies,
        BTreeMap::from([
            (key("tv_dated_post", "createdAt"), TemporalKeyRemedy::TviewRowTimestamp),
            (key("tv_dated_post", "publishedAt"), TemporalKeyRemedy::AddToTview),
            // pg_tviews is installed, but its catalog does not list this table.
            (key("tv_plain", "createdAt"), TemporalKeyRemedy::AddToTable),
            (key("tv_plain", "publishedAt"), TemporalKeyRemedy::AddToTable),
            (key("v_post", "createdAt"), TemporalKeyRemedy::ExposeFromView),
            (key("v_post", "publishedAt"), TemporalKeyRemedy::ExposeFromView),
        ])
    );
    let row_timestamp = messages
        .iter()
        .find(|m| {
            m.contains("`tv_dated_post.created_at`") || m.contains(".tv_dated_post.created_at`")
        })
        .unwrap_or_else(|| panic!("{messages:?}"));
    assert!(!row_timestamp.contains("give the query an argument"), "{row_timestamp}");
}
