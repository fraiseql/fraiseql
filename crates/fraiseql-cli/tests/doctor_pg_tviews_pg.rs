//! #1392: `fraiseql doctor --against-db` reports the physical health of the TVIEWs the schema
//! reads, from `tviews.pg_tviews_profile()`.
//!
//! `tv_book` has the two defects the issue names: a GIN index on `data` no reader uses (it
//! costs every refresh) and an `fk_author` with no leading index (every cascade from an
//! author scans the table). `tv_author` is healthy: LOGGED, no `fk_*`, no index on `data`.
//! A TVIEW is recognised from `pg_tviews`' catalog, through the view the schema reads, never by
//! its name.
//!
//! Needs `pg_tviews`: the `postgres-tviews-test` service, through `TVIEWS_DATABASE_URL`.
//!
//! **Execution engine:** `PostgreSQL` + `pg_tviews` · **Infrastructure:**
//! `TVIEWS_DATABASE_URL` · **Parallelism:** drops and recreates its own `p1392` schema → run
//! `--test-threads=1`.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use fraiseql_cli::commands::doctor::{CheckStatus, pg_tviews_health_checks};
use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};

const SCHEMA: &str = "p1392";

fn tviews_database_url() -> Option<String> {
    std::env::var("TVIEWS_DATABASE_URL").ok().filter(|u| !u.is_empty())
}

async fn provision(url: &str) {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "CREATE EXTENSION IF NOT EXISTS pg_tviews;
             SET search_path = {SCHEMA}, public;
             DROP VIEW IF EXISTS v_book, v_author;
             RESET search_path;
             DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             SET search_path = {SCHEMA}, public;
             CREATE TABLE tb_author (pk_author bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE, name text NOT NULL);
             CREATE TABLE tb_book (pk_book bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
               fk_author bigint NOT NULL REFERENCES tb_author, title text NOT NULL);
             INSERT INTO tb_author (name) VALUES ('ada');
             INSERT INTO tb_book (fk_author, title) SELECT pk_author, 'first' FROM tb_author;
             CREATE TABLE tv_author AS SELECT a.pk_author, a.id,
               jsonb_build_object('id', a.id, 'name', a.name) AS data FROM tb_author a;
             CREATE TABLE tv_book AS SELECT b.pk_book, b.id, b.fk_author,
               jsonb_build_object('id', b.id, 'title', b.title) AS data FROM tb_book b;
             SELECT tviews.pg_tviews_set_logged('author', true);
             SELECT tviews.pg_tviews_set_logged('book', true);
             DO $$ DECLARE i text; BEGIN
               FOR i IN SELECT indexname FROM pg_indexes WHERE schemaname = '{SCHEMA}'
                 AND tablename = 'tv_book' AND indexdef LIKE '%(fk_author%' LOOP
                 EXECUTE format('DROP INDEX {SCHEMA}.%I', i);
               END LOOP; END $$;
             CREATE INDEX tv_book_data_gin ON tv_book USING gin (data);
             CREATE VIEW v_author AS SELECT id, data FROM tv_author;
             CREATE VIEW v_book AS SELECT id, data FROM tv_book;
             ANALYZE tv_author; ANALYZE tv_book;
             RESET search_path;"
        ))
        .await
        .unwrap();
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    for (name, view) in [("Book", "v_book"), ("Author", "v_author")] {
        let mut ty = TypeDefinition::new(name, format!("{SCHEMA}.{view}"));
        ty.fields = vec![FieldDefinition::new("id", FieldType::Id)];
        schema.types.push(ty);
    }
    schema
}

#[tokio::test]
async fn doctor_reports_each_tviews_warnings_and_passes_a_healthy_one() {
    let Some(url) = tviews_database_url() else {
        eprintln!("skipping #1392: TVIEWS_DATABASE_URL not set");
        return;
    };
    provision(&url).await;
    let tls = fraiseql_db::postgres::PostgresTlsConfig::default();
    let checks = pg_tviews_health_checks(&url, &tls, &schema()).await;
    let warned: Vec<&str> = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Warn)
        .map(|c| c.detail.as_str())
        .collect();
    assert!(
        warned
            .iter()
            .any(|d| d.contains("tv_book") && d.contains("pg_tviews_ensure_propagation_indexes")),
        "the missing propagation index: {warned:?}"
    );
    assert!(
        warned.iter().any(|d| d.contains("tv_book") && d.contains("GIN")),
        "the unused GIN on data: {warned:?}"
    );
    assert!(!warned.iter().any(|d| d.contains("tv_author")), "a healthy TVIEW: {warned:?}");
    // `doctor --against-db` runs it.
    let mut file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    std::io::Write::write_all(&mut file, serde_json::to_string(&schema()).unwrap().as_bytes())
        .unwrap();
    let from_doctor =
        fraiseql_cli::commands::doctor::against_db_checks(&url, &tls, file.path(), &[])
            .await
            .into_iter()
            .filter(|c| c.name == "pg_tviews health" && c.status == CheckStatus::Warn)
            .count();
    assert_eq!(from_doctor, warned.len(), "doctor --against-db reports them");
    assert!(checks.iter().all(|c| c.status != CheckStatus::Fail), "{checks:?}");
}

/// A schema that reads no TVIEW gets no `pg_tviews` check at all.
#[tokio::test]
async fn a_schema_reading_no_tview_gets_no_check() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let mut schema = CompiledSchema::new();
    schema.types.push(TypeDefinition::new("Other", "pg_catalog.pg_class"));
    let tls = fraiseql_db::postgres::PostgresTlsConfig::default();
    assert!(pg_tviews_health_checks(&url, &tls, &schema).await.is_empty());
}

/// A query paged by `data ->> 'id'` with nothing indexing it, over `view`.
fn paged_over(view: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut query = fraiseql_core::schema::QueryDefinition::new("books", "Book");
    query.sql_source = Some(view.to_string());
    query.returns_list = true;
    query.pagination_order = Some(fraiseql_core::schema::PaginationOrder::JsonIdentity);
    schema.queries.push(query);
    schema
}

async fn pagination_hints(url: &str, schema: &CompiledSchema) -> Vec<String> {
    use std::io::Write;
    let mut file = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    file.write_all(serde_json::to_string(schema).unwrap().as_bytes()).unwrap();
    file.flush().unwrap();
    let tls = fraiseql_db::postgres::PostgresTlsConfig::default();
    fraiseql_cli::commands::doctor::against_db_checks(url, &tls, file.path(), &[])
        .await
        .into_iter()
        .filter(|c| c.name == "Pagination index" && c.status == CheckStatus::Warn)
        .map(|c| c.hint.unwrap_or_default())
        .collect()
}

/// An index advised on `data` of a TVIEW states its write cost: every refresh rewrites
/// `data`, so such an index disables HOT updates, and the promoted column is the
/// alternative. The same advice on a plain table does not.
#[tokio::test]
async fn index_advice_on_a_tviews_data_states_the_hot_cost() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "CREATE TABLE {SCHEMA}.tb_plain (id uuid, data jsonb);
             CREATE VIEW {SCHEMA}.v_plain AS SELECT id, data FROM {SCHEMA}.tb_plain;"
        ))
        .await
        .unwrap();

    let on_tview = pagination_hints(&url, &paged_over(&format!("{SCHEMA}.v_book"))).await;
    assert!(!on_tview.is_empty(), "advice on the TVIEW");
    assert!(
        on_tview.iter().all(|h| h.contains("HOT") && h.contains("structural column")),
        "{on_tview:?}"
    );
    let on_table = pagination_hints(&url, &paged_over(&format!("{SCHEMA}.v_plain"))).await;
    assert!(!on_table.is_empty(), "advice on the table");
    assert!(on_table.iter().all(|h| !h.contains("HOT")), "{on_table:?}");
}
