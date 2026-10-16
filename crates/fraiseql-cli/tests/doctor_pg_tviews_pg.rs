//! #1392: `fraiseql doctor --against-db` reports the physical health of the TVIEWs the schema
//! reads, from `tviews.pg_tviews_profile()`.
//!
//! `tv_book` has the two defects the issue names: a GIN index on `data` no reader uses (it
//! costs every refresh) and an `fk_author` with no leading index (every cascade from an
//! author scans the table). It embeds its author's name, so that cascade exists: `pg_tviews`
//! only asks for the index of a column a write propagates through. `tv_author` is healthy: LOGGED,
//! no `fk_*`, no index on `data`. A TVIEW is recognised from `pg_tviews`' catalog, through the view
//! the schema reads, never by its name.
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
               jsonb_build_object('id', b.id, 'title', b.title,
                 'author', jsonb_build_object('name', a.name)) AS data
               FROM tb_book b JOIN tb_author a ON a.pk_author = b.fk_author;
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

/// A query paged by its derived identity with nothing indexing it, over `view`.
fn paged_over(view: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut query = fraiseql_core::schema::QueryDefinition::new("books", "Book");
    query.sql_source = Some(view.to_string());
    query.returns_list = true;
    query.pagination_order = Some(fraiseql_core::schema::PaginationOrder::Identity);
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

/// Pagination advice over a TVIEW proposes an index on its `id` column, never one on `data`:
/// the derived identity is the native column (#1569), so the advice no longer costs a TVIEW
/// its HOT updates (#1392).
#[tokio::test]
async fn index_advice_on_a_tview_proposes_the_id_column_not_data() {
    let Some(url) = tviews_database_url() else {
        return;
    };
    provision(&url).await;

    let on_tview = pagination_hints(&url, &paged_over(&format!("{SCHEMA}.v_book"))).await;
    assert!(!on_tview.is_empty(), "advice on the TVIEW");
    assert!(
        on_tview.iter().all(|h| h.ends_with("(id);") && !h.contains("data")),
        "{on_tview:?}"
    );
}

/// With read replicas, a schema reading an UNLOGGED TVIEW is refused at boot, and the
/// refusal prints the statements that lift it (#1598): run as printed, they make the table
/// LOGGED, `pg_tviews` records it as its `logged` option, and the same schema boots. Two
/// views read the one TVIEW: one statement for it.
#[tokio::test]
async fn the_replica_refusal_prints_the_statements_that_lift_it() {
    use fraiseql_core::{
        db::postgres::{
            PoolPrewarmConfig, PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy, SearchPath,
            VectorScanConfig,
        },
        schema::{QueryDefinition, refuse_standby_unreadable_sources},
    };

    const UNLOGGED: &str = "p1598";
    let Some(url) = tviews_database_url() else {
        return;
    };
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(&format!(
            "CREATE EXTENSION IF NOT EXISTS pg_tviews;
             DROP VIEW IF EXISTS {UNLOGGED}.v_draft, {UNLOGGED}.v_draft_again;
             DO $$ DECLARE t text; BEGIN
               FOR t IN SELECT schema || '.' || name FROM tviews.registry
                        WHERE schema = '{UNLOGGED}' LOOP
                 PERFORM tviews.pg_tviews_drop(t);
               END LOOP; END $$;
             DROP SCHEMA IF EXISTS {UNLOGGED} CASCADE;
             CREATE SCHEMA {UNLOGGED};
             CREATE TABLE {UNLOGGED}.tb_draft (pk_draft bigint GENERATED ALWAYS AS IDENTITY
               PRIMARY KEY, id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE);
             SELECT tviews.pg_tviews_create('{UNLOGGED}.tv_draft', $T$
               SELECT d.pk_draft, d.id, jsonb_build_object('id', d.id) AS data
               FROM {UNLOGGED}.tb_draft d $T$, '{{\"logged\": false}}');
             CREATE VIEW {UNLOGGED}.v_draft AS SELECT id, data FROM {UNLOGGED}.tv_draft;
             CREATE VIEW {UNLOGGED}.v_draft_again AS SELECT id, data FROM {UNLOGGED}.tv_draft;"
        ))
        .await
        .unwrap();
    let mut schema = CompiledSchema::new();
    for (name, view) in [("drafts", "v_draft"), ("draftsAgain", "v_draft_again")] {
        let mut query = QueryDefinition::new(name, "Draft");
        query.sql_source = Some(format!("{UNLOGGED}.{view}"));
        schema.queries.push(query);
    }
    // Reads routed to a replica: the refusal is decided on the primary's catalog, so the
    // replica may be the same database. The server resolves names in its own schema; the
    // statements must not depend on that.
    let adapter = PostgresAdapter::with_pool_config(
        &url,
        PoolPrewarmConfig {
            min_size:            0,
            max_size:            2,
            timeout_secs:        None,
            search_path:         Some(SearchPath::new([UNLOGGED]).unwrap()),
            max_streaming_reads: None,
            vector_scan:         VectorScanConfig::default(),
            tls:                 PostgresTlsConfig::default(),
            read_replicas:       ReadReplicaPolicy::default().with_urls(vec![url.clone()]),
        },
    )
    .await
    .unwrap();

    let message = refuse_standby_unreadable_sources(&adapter, &schema)
        .await
        .expect_err("an UNLOGGED TVIEW behind a replica")
        .to_string();
    let statements: Vec<&str> = message
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("ALTER TABLE"))
        .collect();
    assert_eq!(statements.len(), 1, "one statement for the one table: {message}");
    for statement in statements {
        client.batch_execute(statement).await.unwrap();
    }

    refuse_standby_unreadable_sources(&adapter, &schema)
        .await
        .expect("the printed statements lift the refusal");
    let logged: Option<bool> = client
        .query_one(
            "SELECT (options ->> 'logged')::boolean FROM tviews.registry \
             WHERE schema = $1 AND name = 'tv_draft'",
            &[&UNLOGGED],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(logged, Some(true), "pg_tviews records the TVIEW as LOGGED");
}
