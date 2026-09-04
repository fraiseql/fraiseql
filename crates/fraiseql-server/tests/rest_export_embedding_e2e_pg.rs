//! #1268 regression: a streaming export refuses a `?select=` it cannot honour, rather
//! than validating it and emitting rows without it.
//!
//! The three export representations — NDJSON, CSV, XLSX — resolve through
//! `resolve_streaming_get_query`, which fills `params.embeddings` and
//! `params.embedding_counts`, depth-validates them and confirms every named relationship
//! exists. All three then destructured `ResolvedGetQuery { query_match, variables, .. }`
//! and dropped the rest of `params` on the floor: `execute_embeddings` has one caller, the
//! JSON path. So a validated selection was silently reduced, under a 200.
//!
//! The two visible shapes:
//!
//! * **NDJSON** answered `{"id":10}` to `?select=id,author(name)`. The identical request as JSON
//!   answers `{"id":10,"author":{"name":"alice"}}`.
//! * **CSV and XLSX** are worse, because both build their header row from the raw `?select=`
//!   (`parse_select_top_level`, paren-aware). The export therefore carried a column literally named
//!   `author` that was empty on every row — indistinguishable from a table where no post has an
//!   author.
//!
//! **The decision this pins: refuse.** An export is documented as *one statement over one
//! database portal* — a single snapshot, `O(N)` in row scans, holding one pooled connection
//! from the first row to the last (`docs/operations/graphql-sse-streaming.md`,
//! `docs/runbooks/07-connection-pool-exhaustion.md`). An embed, as this engine resolves one,
//! is a per-parent-row sub-query issued on a *second* connection (`embed_into_rows` loops
//! `embed_into_single`; `execute_embedding_counts` loops `count_related`). Executing embeds
//! on an export would break every one of those four properties at once, and the export is
//! bounded by nothing — `export_rows` removes `limit` because "its absence means the whole
//! table, which is what an export is for" (#811). The JSON path can afford the same embed
//! because `max_page_size` bounds its parent rows; the export path has no such bound.
//!
//! What is *not* an option is the third one that shipped: accept, validate, discard.
//!
//! **Why a real database.** Every assertion here is about a 200 response's content, or about
//! a refusal arriving as an HTTP status rather than as an error line inside a body that has
//! already begun. The JSON control in particular has to resolve a real embed off real rows,
//! otherwise "the export refuses" and "embedding is broken" look identical.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p13_export_embed` schema → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

/// The CSV and XLSX halves of this suite measured the loudest shape of #1268 — a named,
/// permanently empty column — and both are `#[cfg]`-gated on their export feature. A
/// `.dagger/main.go` line that loses `--features export-csv,export-xlsx` would drop four
/// cases and still print `ok`, which is the shape `-- --ignored` over a suite with no
/// `#[ignore]` already cost once.
///
/// A runtime failure rather than a `compile_error!`: the feature matrix deliberately
/// builds `rest,export-csv` and `rest,export-xlsx` separately (#920), and a build-time
/// refusal here would redden those combos the day either gains `--all-targets`.
#[cfg(not(all(feature = "export-csv", feature = "export-xlsx")))]
#[test]
fn the_csv_and_xlsx_cases_must_be_compiled_in() {
    panic!(
        "rest_export_embedding_e2e_pg was built without export-csv/export-xlsx: its CSV \
         and XLSX cases did not run. Restore `--features rest,export-csv,export-xlsx` on \
         this binary's invocation."
    );
}

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldType, QueryDefinition, Relationship,
        RestConfig, TypeDefinition,
    },
};
use fraiseql_server::routes::{
    graphql::AppState,
    rest::{RestMountConfig, rest_query_router},
};
use fraiseql_test_support::try_database_url;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

const SCHEMA: &str = "p13_export_embed";

const NDJSON: &str = "application/x-ndjson";
#[cfg(feature = "export-csv")]
const CSV: &str = "text/csv";
#[cfg(feature = "export-xlsx")]
const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

// ---------------------------------------------------------------------------
// Fixture: two authors, two posts each, one orphaned post
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_author (id bigint PRIMARY KEY, name text NOT NULL)"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_post (id bigint PRIMARY KEY, fk_author bigint, \
             title text NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_author VALUES (1, 'alice'), (2, 'bob')"),
        format!(
            "INSERT INTO {SCHEMA}.tb_post VALUES (10, 1, 'a-one'), (11, 1, 'a-two'), \
             (20, 2, 'b-one'), (21, 2, 'b-two')"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_author AS SELECT id, jsonb_build_object('id', id, 'name', \
             name) AS data FROM {SCHEMA}.tb_author ORDER BY id"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_post AS SELECT id, jsonb_build_object('id', id, 'fk_author', \
             fk_author, 'title', title) AS data FROM {SCHEMA}.tb_post ORDER BY id"
        ),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn list_query(name: &str, ty: &str, view: &str, streamable: bool) -> QueryDefinition {
    let mut q = QueryDefinition::new(name, ty)
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.{view}"));
    q.auto_params.has_where = true;
    q.auto_params.has_limit = true;
    q.rest_stream = streamable;
    q
}

/// The `author` relationship, declared identically on `Post` and on `Draft`.
fn author_relationship() -> Relationship {
    Relationship {
        name:           "author".to_string(),
        target_type:    "Author".to_string(),
        cardinality:    Cardinality::ManyToOne,
        foreign_key:    "fk_author".to_string(),
        referenced_key: "id".to_string(),
    }
}

/// `authors` and `posts` are streamable; `drafts` reads the same view through its own
/// type and is **not**, so the `406` for "this route offers no stream at all" has a
/// subject otherwise identical to `posts`. Without it, an ordering regression that
/// answered `400` before checking `rest_stream` would have nothing to fail against.
///
/// `Draft` is a distinct *type* rather than a second query on `Post` because the route
/// table groups resources by return type and names each after its first list query: two
/// list queries on `Post` both claim `GET /posts`, `detect_conflicts` refuses the schema,
/// and `rest_query_router` returns `None` — a rig that builds no server at all.
fn schema_fixture() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut author = TypeDefinition::new("Author", format!("{SCHEMA}.v_author"));
    author.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("name", FieldType::String),
    ];
    author.relationships = vec![Relationship {
        name:           "posts".to_string(),
        target_type:    "Post".to_string(),
        cardinality:    Cardinality::OneToMany,
        foreign_key:    "fk_author".to_string(),
        referenced_key: "id".to_string(),
    }];
    schema.types.push(author);

    let mut post = TypeDefinition::new("Post", format!("{SCHEMA}.v_post"));
    post.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("fk_author", FieldType::Int),
        FieldDefinition::new("title", FieldType::String),
    ];
    post.relationships = vec![author_relationship()];
    schema.types.push(post);

    let mut draft = TypeDefinition::new("Draft", format!("{SCHEMA}.v_post"));
    draft.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("fk_author", FieldType::Int),
        FieldDefinition::new("title", FieldType::String),
    ];
    draft.relationships = vec![author_relationship()];
    schema.types.push(draft);

    schema.queries.push(list_query("authors", "Author", "v_author", true));
    schema.queries.push(list_query("posts", "Post", "v_post", true));
    schema.queries.push(list_query("drafts", "Draft", "v_post", false));

    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

struct Rig {
    router: axum::Router,
}

/// A response reduced to what these assertions are about: the status, and the body as
/// text. NDJSON and CSV are not JSON documents, and an XLSX body is a ZIP — only the
/// refusal bodies parse, so the text is what every case can share.
struct Res {
    status: StatusCode,
    ctype:  String,
    body:   String,
}

impl Res {
    /// The `error.message` of a refusal body, or the raw text when it is not one.
    ///
    /// Assertions read the *message*, not just the status: a `400` proves only that
    /// something refused, and this suite has several branches that can each produce one.
    fn message(&self) -> String {
        serde_json::from_str::<Value>(&self.body)
            .ok()
            .and_then(|v| v.get("error")?.get("message")?.as_str().map(ToString::to_string))
            .unwrap_or_else(|| self.body.clone())
    }
}

impl Rig {
    async fn get(&self, uri: &str, accept: &str) -> Res {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("accept", accept)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let ctype = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        Res {
            status,
            ctype,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        }
    }
}

async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let executor = Arc::new(Executor::new(schema_fixture(), adapter));
    let state = AppState::new(executor);
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");

    Some(Rig { router })
}

macro_rules! rig_or_skip {
    () => {
        match rig().await {
            Some(rig) => rig,
            None => {
                eprintln!("skipping: DATABASE_URL not set");
                return;
            },
        }
    };
}

// ---------------------------------------------------------------------------
// The refusal, on each representation
// ---------------------------------------------------------------------------

/// NDJSON: `?select=id,author(name)` used to answer `200` with `{"id":10}` per line.
///
/// The refusal has to arrive as an HTTP status with a JSON error body, not as the
/// `{"error":"…"}` line the streaming body uses for mid-stream failures — the row source
/// has not opened yet, so this is still an ordinary request error (#958).
#[tokio::test]
async fn an_ndjson_export_refuses_an_embedded_relationship() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=id,author(name)", NDJSON).await;

    assert_eq!(
        res.status,
        StatusCode::BAD_REQUEST,
        "an embed the export cannot carry must be refused, not dropped from the rows: {}",
        res.body
    );
    assert!(
        res.ctype.starts_with("application/json"),
        "the refusal is a request error, not a stream that has begun: {} / {}",
        res.ctype,
        res.body
    );
    let message = res.message();
    assert!(
        message.contains("author"),
        "the refusal must name the relationship it refused: {message}"
    );
    assert!(
        message.contains("embedded relationship"),
        "the refusal must say which selection it refused, so it is distinguishable from \
         the count refusal and from the pagination one: {message}"
    );
}

/// CSV: the loudest shape. `parse_select_top_level` puts `author` in the header row, so
/// the export carried a named column that was empty on every row.
#[tokio::test]
#[cfg(feature = "export-csv")]
async fn a_csv_export_refuses_an_embedded_relationship() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=id,author(name)", CSV).await;

    assert_eq!(
        res.status,
        StatusCode::BAD_REQUEST,
        "CSV used to emit a header naming `author` and an empty cell under it on every \
         row: {}",
        res.body
    );
    assert!(
        !res.body.contains("id,author"),
        "no header row may be emitted for a selection the export refuses: {}",
        res.body
    );
    let message = res.message();
    assert!(
        message.contains("author") && message.contains("embedded relationship"),
        "the refusal must name the relationship and the selection kind: {message}"
    );
}

/// XLSX: same header construction, same empty column, and the workbook is buffered whole
/// before any of it is sent — so there is no reason at all for the refusal to be late.
#[tokio::test]
#[cfg(feature = "export-xlsx")]
async fn an_xlsx_export_refuses_an_embedded_relationship() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=id,author(name)", XLSX).await;

    assert_eq!(
        res.status,
        StatusCode::BAD_REQUEST,
        "the workbook must not be built at all for a selection it cannot carry: {}",
        res.body
    );
    assert!(
        !res.body.starts_with("PK"),
        "a refusal must not answer with a ZIP container: {} bytes",
        res.body.len()
    );
    let message = res.message();
    assert!(
        message.contains("author") && message.contains("embedded relationship"),
        "the refusal must name the relationship and the selection kind: {message}"
    );
}

/// The count half of the same defect: `?select=id,posts.count` was validated —
/// `validate_embedding_relationship_name` confirms `posts` exists — and then the export
/// emitted rows with no `posts_count` key.
///
/// Its diagnosis has to differ from the embed one. `embeddings` and `embedding_counts` are
/// separate fields filled by separate `?select=` syntaxes, so a single shared message would
/// let either branch be deleted while the other kept the suite green.
#[tokio::test]
async fn an_ndjson_export_refuses_an_embedded_count() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/authors?select=id,posts.count", NDJSON).await;

    assert_eq!(
        res.status,
        StatusCode::BAD_REQUEST,
        "a count the export cannot carry must be refused, not dropped: {}",
        res.body
    );
    let message = res.message();
    assert!(
        message.contains("posts"),
        "the refusal must name the relationship it refused: {message}"
    );
    assert!(
        message.contains("embedded count"),
        "the count branch must state its own diagnosis, not the embed branch's: {message}"
    );
}

#[tokio::test]
#[cfg(feature = "export-csv")]
async fn a_csv_export_refuses_an_embedded_count() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/authors?select=id,posts.count", CSV).await;

    assert_eq!(res.status, StatusCode::BAD_REQUEST, "{}", res.body);
    assert!(res.message().contains("embedded count"), "{}", res.message());
}

/// The shape that loses the most: a `?select=` naming *only* an embed.
///
/// `parse_select_with_embeddings` returns `RestFieldSpec::All` when the flat list is empty
/// and something is embedded, so `?select=author(name)` asked for one thing and the export
/// answered with every column of the parent and no author at all.
#[tokio::test]
async fn an_export_refuses_a_select_that_names_only_an_embed() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=author(name)", NDJSON).await;

    assert_eq!(
        res.status,
        StatusCode::BAD_REQUEST,
        "a select naming only an embed used to widen to every parent column: {}",
        res.body
    );
    assert!(!res.body.contains("\"title\""), "no rows may be emitted: {}", res.body);
}

// ---------------------------------------------------------------------------
// Controls — the refusal must be about the selection, not about the export
// ---------------------------------------------------------------------------

/// The same embed on the JSON representation still resolves against real rows.
///
/// Without this, every assertion above passes just as well on a build where embedding is
/// broken outright, and the refusal would read as a fix for a defect it did not cause.
#[tokio::test]
async fn the_same_embed_still_resolves_on_the_json_representation() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=id,author(name)", "application/json").await;

    assert_eq!(res.status, StatusCode::OK, "JSON must still embed: {}", res.body);
    let body: Value = serde_json::from_str(&res.body).expect("JSON body");
    let rows = body["data"].as_array().expect("data array").clone();
    assert_eq!(rows.len(), 4, "four posts: {body}");
    let post_10 = rows.iter().find(|r| r["id"].as_i64() == Some(10)).expect("post 10").clone();
    assert_eq!(post_10["author"]["name"], json!("alice"), "post 10's author: {body}");
    assert!(
        post_10.get("fk_author").is_none(),
        "the join key the server projected for itself is still stripped (#1230): {body}"
    );
}

/// An export with no embed in its `?select=` streams exactly as before.
#[tokio::test]
async fn an_ndjson_export_without_an_embed_still_streams() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=id,title", NDJSON).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let lines: Vec<&str> = res.body.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 4, "one line per post: {}", res.body);
    let first: Value = serde_json::from_str(lines[0]).expect("NDJSON line");
    assert_eq!(first["id"], json!(10), "{}", res.body);
    assert_eq!(first["title"], json!("a-one"), "{}", res.body);
}

#[tokio::test]
#[cfg(feature = "export-csv")]
async fn a_csv_export_without_an_embed_still_streams() {
    let rig = rig_or_skip!();

    let res = rig.get("/rest/v1/posts?select=id,title", CSV).await;

    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert!(res.body.contains("id,title"), "header row: {}", res.body);
    assert!(res.body.contains("10,a-one"), "first data row: {}", res.body);
}

/// A field whose *name* merely contains an embedded relationship's name is not an embed.
///
/// The refusal reads `params.embeddings` / `params.embedding_counts`, which the extractor
/// fills only from the parenthetical and `.count` syntaxes. A refusal implemented by
/// grepping the raw `?select=` string for a relationship name would fail here.
#[tokio::test]
async fn a_flat_field_is_not_mistaken_for_an_embed() {
    let rig = rig_or_skip!();

    // `fk_author` contains `author`, the name of Post's relationship.
    let res = rig.get("/rest/v1/posts?select=id,fk_author", NDJSON).await;

    assert_eq!(
        res.status,
        StatusCode::OK,
        "selecting a flat column is not embedding: {}",
        res.body
    );
    let first: Value =
        serde_json::from_str(res.body.trim_end().split('\n').next().unwrap()).expect("NDJSON");
    assert_eq!(first["fk_author"], json!(1), "{}", res.body);
}

/// Ordering: a route that offers no stream at all answers `406`, and keeps answering it
/// when the request also carries an embed.
///
/// `resolve_streaming_get_query` refuses an unstreamable route before it looks at the
/// selection, and it has to stay that way: "this route has no such representation" is the
/// more fundamental refusal, and swapping the two would tell a client to fix its
/// `?select=` on a route where no `?select=` would have worked.
#[tokio::test]
async fn a_route_without_rest_stream_still_answers_406_before_the_embed_refusal() {
    let rig = rig_or_skip!();

    let with_embed = rig.get("/rest/v1/drafts?select=id,author(name)", NDJSON).await;
    assert_eq!(
        with_embed.status,
        StatusCode::NOT_ACCEPTABLE,
        "the route offers no stream, which is the refusal that applies: {}",
        with_embed.body
    );
    assert!(
        with_embed.message().contains("rest_stream"),
        "the 406 keeps its own diagnosis: {}",
        with_embed.message()
    );

    // And the same route serves the embed perfectly well as JSON — `rest_stream` gates
    // the representation, not the selection.
    let as_json = rig.get("/rest/v1/drafts?select=id,author(name)", "application/json").await;
    assert_eq!(as_json.status, StatusCode::OK, "{}", as_json.body);
}
