//! #863 / #864 regression: an embedded collection must belong to its parent, and a
//! validated nesting depth must actually execute.
//!
//! * **#863** `embed_into_single` seeded the sub-query `WHERE` map with the parent join predicate
//!   and then merged the client's `?rel.field[op]=value` filter over it with
//!   `serde_json::Map::insert`, which **replaces**. A filter naming the join column destroyed the
//!   parent scoping, so one parent's record came back advertising another parent's children as its
//!   own. The conventional `referenced_key` for `ManyToOne`/`OneToOne` is `id`, so
//!   `?author.id[gt]=0` — a natural thing for a client to write — was enough.
//! * **#864** `execute_embeddings` collected only `SelectEntry::Field` from a spec's sub-select, so
//!   nested `Embedded` entries were parsed, depth-validated against `max_embedding_depth` (default
//!   3, documented as `?select=posts(comments)`), and then silently discarded. The response carried
//!   no `comments` key at all, and a client could not distinguish "no comments" from "the server
//!   dropped my selection".
//!
//! **Why a real database.** Both defects are wrong-*answer* bugs on a 200 response. #863
//! returns real rows belonging to the wrong parent; #864 returns a well-formed object with
//! a key missing. Neither produces an error, so only asserting the response *content*
//! against known seeded data can see them.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p13_embed` schema → run `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

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

const SCHEMA: &str = "p13_embed";

// ---------------------------------------------------------------------------
// Fixture: two authors, two posts each, two comments per post
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_author (id bigint PRIMARY KEY, name text NOT NULL)"),
        // `fk_author` is NULLABLE so that "this post has no author" is a state the
        // fixture can actually hold. #1230 turned an unprojected join key into the
        // *same* answer as a null one; a test that cannot produce a genuine null
        // cannot show that the two are distinguishable again.
        format!(
            "CREATE TABLE {SCHEMA}.tb_post (id bigint PRIMARY KEY, fk_author bigint, \
             title text NOT NULL)"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_comment (id bigint PRIMARY KEY, fk_post bigint NOT NULL, \
             body text NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_author VALUES (1, 'alice'), (2, 'bob')"),
        // Author 1 owns posts 10,11; author 2 owns posts 20,21; post 30 is orphaned.
        format!(
            "INSERT INTO {SCHEMA}.tb_post VALUES (10, 1, 'a-one'), (11, 1, 'a-two'), \
             (20, 2, 'b-one'), (21, 2, 'b-two'), (30, NULL, 'orphan')"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_comment VALUES (100, 10, 'c-a1'), (101, 10, 'c-a2'), \
             (200, 20, 'c-b1')"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_author AS SELECT id, jsonb_build_object('id', id, 'name', \
             name) AS data FROM {SCHEMA}.tb_author ORDER BY id"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_post AS SELECT id, jsonb_build_object('id', id, 'fk_author', \
             fk_author, 'title', title) AS data FROM {SCHEMA}.tb_post ORDER BY id"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_comment AS SELECT id, jsonb_build_object('id', id, 'fk_post', \
             fk_post, 'body', body) AS data FROM {SCHEMA}.tb_comment ORDER BY id"
        ),
    ];

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// `client_where` is the target query's `auto_params.has_where` — the flag that
/// governs the **client-facing filter surface** (`[query_defaults] where`, or a
/// per-query override).
///
/// #1170: the server's own parent-scoping predicate must not depend on it. A
/// project that turns a query's `where` argument off is saying "clients may not
/// filter this"; it is not saying "and relations that embed it may go unscoped".
fn list_query(name: &str, ty: &str, view: &str, client_where: bool) -> QueryDefinition {
    let mut q = QueryDefinition::new(name, ty)
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.{view}"));
    q.auto_params.has_where = client_where;
    q.auto_params.has_limit = true;
    q
}

fn schema_with(embedded_client_where: bool) -> CompiledSchema {
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
    post.relationships = vec![
        Relationship {
            name:           "comments".to_string(),
            target_type:    "Comment".to_string(),
            cardinality:    Cardinality::OneToMany,
            foreign_key:    "fk_post".to_string(),
            referenced_key: "id".to_string(),
        },
        // The ManyToOne direction, so `embed_into_single`'s *object* branch has a
        // subject: it takes the first row of the target query's result, so an
        // unscoped embed does not merely over-return — it attributes the wrong
        // parent (#1170).
        Relationship {
            name:           "author".to_string(),
            target_type:    "Author".to_string(),
            cardinality:    Cardinality::ManyToOne,
            foreign_key:    "fk_author".to_string(),
            referenced_key: "id".to_string(),
        },
    ];
    schema.types.push(post);

    let mut comment = TypeDefinition::new("Comment", format!("{SCHEMA}.v_comment"));
    comment.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("fk_post", FieldType::Int),
        FieldDefinition::new("body", FieldType::String),
    ];
    schema.types.push(comment);

    // Every list query takes the flag, `authors` included: it is the *target* of
    // `Post.author`, so leaving it on would let the ManyToOne test pass while
    // testing nothing.
    schema
        .queries
        .push(list_query("authors", "Author", "v_author", embedded_client_where));
    schema
        .queries
        .push(list_query("posts", "Post", "v_post", embedded_client_where));
    schema
        .queries
        .push(list_query("comments", "Comment", "v_comment", embedded_client_where));

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

impl Rig {
    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
        (status, json)
    }
}

async fn rig() -> Option<Rig> {
    rig_with(true).await
}

/// A rig whose *embedded* target queries accept a client `where` or do not.
async fn rig_with(embedded_client_where: bool) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let executor = Arc::new(Executor::new(schema_with(embedded_client_where), adapter));
    let state = AppState::new(executor);
    let router = rest_query_router(&state, &RestMountConfig::default()).expect("REST router");

    Some(Rig { router })
}

/// The `posts` array of the author row with `id == author_id`.
fn posts_of(body: &Value, author_id: i64) -> Vec<Value> {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(author_id))
        })
        .and_then(|row| row.get("posts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn titles(posts: &[Value]) -> Vec<String> {
    posts
        .iter()
        .filter_map(|p| p.get("title").and_then(Value::as_str).map(ToString::to_string))
        .collect()
}

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

/// Baseline: without a client filter, each author gets exactly their own posts.
///
/// Stated separately so a failure of the #863 test below can be read as "the filter broke
/// the scoping" rather than "embedding never worked".
#[tokio::test]
async fn an_embedded_collection_belongs_to_its_parent() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(id,title)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(titles(&posts_of(&body, 1)), vec!["a-one", "a-two"], "author 1: {body}");
    assert_eq!(titles(&posts_of(&body, 2)), vec!["b-one", "b-two"], "author 2: {body}");
}

/// #863: a client filter naming the join key must not widen the embedded collection.
///
/// `?posts.fk_author[gt]=0` is true for every post in the table. Before the fix it
/// *replaced* the `fk_author = <parent id>` predicate, so every author's record listed
/// all four posts — real rows, attributed to the wrong parent, with a 200 status.
#[tokio::test]
async fn a_client_filter_cannot_overwrite_the_parent_join_key() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig
        .get("/rest/v1/authors?select=id,posts(id,title)&posts.fk_author[gt]=0")
        .await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(
        titles(&posts_of(&body, 1)),
        vec!["a-one", "a-two"],
        "author 1 must still see only its own posts — a filter on the join key must not \
         replace the parent scoping: {body}"
    );
    assert_eq!(
        titles(&posts_of(&body, 2)),
        vec!["b-one", "b-two"],
        "author 2 must still see only its own posts: {body}"
    );
}

/// #863, the narrowing direction: a legitimate client filter must still apply.
///
/// The fix must compose the two predicates, not ignore the client's — `_and`, not
/// "parent scoping wins outright".
#[tokio::test]
async fn a_client_filter_still_narrows_the_embedded_collection() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig
        .get("/rest/v1/authors?select=id,posts(id,title)&posts.title[eq]=a-one")
        .await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(
        titles(&posts_of(&body, 1)),
        vec!["a-one"],
        "the client filter must narrow author 1's posts: {body}"
    );
    assert!(
        titles(&posts_of(&body, 2)).is_empty(),
        "author 2 has no post titled 'a-one': {body}"
    );
}

/// #864: a depth-2 embedding must execute, not be validated and dropped.
///
/// `max_embedding_depth` defaults to 3 and its documentation is literally
/// `?select=posts(comments)`, so accepting this request and discarding the nested
/// selection is the validator and the executor disagreeing.
#[tokio::test]
async fn a_nested_embedding_executes_to_the_validated_depth() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) =
        rig.get("/rest/v1/authors?select=id,posts(id,title,comments(id,body))").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let posts = posts_of(&body, 1);
    assert_eq!(posts.len(), 2, "author 1 should have two posts: {body}");

    let post_10 = posts
        .iter()
        .find(|p| p.get("id").and_then(Value::as_i64) == Some(10))
        .unwrap_or_else(|| panic!("post 10 missing: {body}"));

    let comments = post_10
        .get("comments")
        .unwrap_or_else(|| panic!("the `comments` key was dropped entirely: {body}"))
        .as_array()
        .unwrap_or_else(|| panic!("`comments` is not an array: {body}"));

    let bodies: Vec<&str> = comments.iter().filter_map(|c| c.get("body")?.as_str()).collect();
    assert_eq!(bodies, vec!["c-a1", "c-a2"], "post 10's own comments: {body}");

    // Scoping must hold at the nested level too — post 11 has no comments, and post 20's
    // comment belongs to the other author entirely.
    let post_11 = posts
        .iter()
        .find(|p| p.get("id").and_then(Value::as_i64) == Some(11))
        .unwrap_or_else(|| panic!("post 11 missing: {body}"));
    assert_eq!(
        post_11.get("comments").and_then(Value::as_array).map(Vec::len),
        Some(0),
        "post 11 has no comments and must report an empty array, not another post's: {body}"
    );
}

// ---------------------------------------------------------------------------
// #1170 — the server's own scoping must not ride on the client filter surface
// ---------------------------------------------------------------------------

/// The `<rel>_count` value on the author row with `id == author_id`.
fn count_of(body: &Value, author_id: i64, key: &str) -> Option<i64> {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(author_id))
        })
        .and_then(|row| row.get(key))
        .and_then(Value::as_i64)
}

/// The `author` object of the post row with `id == post_id`.
fn author_of(body: &Value, post_id: i64) -> Option<String> {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(post_id)))
        .and_then(|row| row.get("author"))
        .and_then(|a| a.get("name"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// #1170: a target query that does not accept a client `where` must still be
/// **scoped to its parent**.
///
/// The predicate was built into `arguments["where"]` and composed only when the
/// target's `auto_params.has_where` was set, so turning off a query's
/// client-facing filter argument also turned off relation scoping for every
/// parent embedding it — silently, on a 200. With four posts in the table and
/// two per author, the wrong answer and the right one differ by construction.
#[tokio::test]
async fn an_embedded_collection_is_scoped_even_when_the_target_forbids_a_client_where() {
    let Some(rig) = rig_with(false).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(id,title)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(
        titles(&posts_of(&body, 1)),
        vec!["a-one", "a-two"],
        "author 1 must see only its own posts even though `posts` declares \
         has_where = false — the join predicate is the server's scoping, not a client \
         filter: {body}"
    );
    assert_eq!(
        titles(&posts_of(&body, 2)),
        vec!["b-one", "b-two"],
        "author 2 must see only its own posts: {body}"
    );
}

/// #1170 on the count path (`count_rows`), which is the shape the issue measured:
/// an unscoped number reported as a relation's cardinality.
#[tokio::test]
async fn an_embedded_count_is_scoped_even_when_the_target_forbids_a_client_where() {
    let Some(rig) = rig_with(false).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts.count").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(
        count_of(&body, 1, "posts_count"),
        Some(2),
        "author 1 owns 2 of the 4 posts; 4 is the whole table reported as this \
         parent's cardinality: {body}"
    );
    assert_eq!(count_of(&body, 2, "posts_count"), Some(2), "author 2 owns 2 posts: {body}");
}

/// #1170 on the `ManyToOne` object branch. Here an unscoped embed does not
/// over-return — `embed_into_single` takes the **first** row of the target
/// query's result — so every post is attributed to author 1.
#[tokio::test]
async fn a_many_to_one_embed_is_scoped_even_when_the_target_forbids_a_client_where() {
    let Some(rig) = rig_with(false).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // `fk_author` is selected deliberately: `extract_join_key` reads the ManyToOne
    // join key off the *parent row*, so a document that does not project it embeds
    // `null` and this test would assert nothing about scoping.
    let (status, body) = rig.get("/rest/v1/posts?select=id,fk_author,author(name)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(author_of(&body, 10).as_deref(), Some("alice"), "post 10 is alice's: {body}");
    assert_eq!(
        author_of(&body, 20).as_deref(),
        Some("bob"),
        "post 20 is bob's — an unscoped target query returns every author and the \
         embed takes the first, which is alice: {body}"
    );
}

/// **Control.** The same three reads with the client `where` surface *on*, so a
/// failure above reads as "scoping depends on the flag" rather than "embedding
/// never worked for counts or `ManyToOne`".
#[tokio::test]
async fn the_same_embeds_are_scoped_when_the_target_does_accept_a_client_where() {
    let Some(rig) = rig_with(true).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (_, body) = rig.get("/rest/v1/authors?select=id,posts.count").await;
    assert_eq!(count_of(&body, 1, "posts_count"), Some(2), "count control: {body}");
    assert_eq!(count_of(&body, 2, "posts_count"), Some(2), "count control: {body}");

    let (_, body) = rig.get("/rest/v1/posts?select=id,fk_author,author(name)").await;
    assert_eq!(author_of(&body, 10).as_deref(), Some("alice"), "ManyToOne control: {body}");
    assert_eq!(author_of(&body, 20).as_deref(), Some("bob"), "ManyToOne control: {body}");
}

/// #1170, the narrowing direction, and the reason the fix is *not* "ignore
/// `has_where` for the whole `where` argument": a client filter on a target that
/// forbids one must still be refused, not quietly applied through the scoping
/// slot. Author 1 owns `a-one` and `a-two`; a filter that would narrow to one of
/// them must not take effect here.
#[tokio::test]
async fn a_client_filter_is_still_inert_on_a_target_that_forbids_a_client_where() {
    let Some(rig) = rig_with(false).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig
        .get("/rest/v1/authors?select=id,posts(id,title)&posts.title[eq]=a-one")
        .await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(
        titles(&posts_of(&body, 1)),
        vec!["a-one", "a-two"],
        "the target does not publish a `where` argument, so the client filter does not \
         apply — but the parent scoping still does: {body}"
    );
}

// ---------------------------------------------------------------------------
// #1230 — the join key is the server's business, not a term of the client's
// contract
// ---------------------------------------------------------------------------

/// The keys of a response row, sorted — the assertions below are about *which*
/// keys a document carries, and `serde_json` preserves insertion order, which is
/// the server's projection order and not part of any contract.
fn keys_of(row: &Value) -> Vec<String> {
    let mut keys: Vec<String> = row.as_object().expect("object row").keys().cloned().collect();
    keys.sort();
    keys
}

/// The parent row carrying `id == id_value`, whatever else is on it.
fn row_of(body: &Value, id_value: i64) -> Value {
    body.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter().find(|r| r.get("id").and_then(Value::as_i64) == Some(id_value))
        })
        .cloned()
        .unwrap_or_else(|| panic!("no row with id {id_value}: {body}"))
}

/// #1230: a `ManyToOne` embed must resolve from a document that names only the
/// relationship.
///
/// `extract_join_key` reads the join key off the already-projected parent row, and
/// for `ManyToOne` that key is the **foreign key** — the one column a client asking
/// for `author` has every reason not to select. Selecting `id,author(name)` returned
/// four posts with `"author": null` under a 200, indistinguishable from four posts
/// that genuinely have no author. Every author exists.
///
/// `OneToMany` hid this: there the key is the parent's `referenced_key`,
/// conventionally `id`, which a client almost always selects anyway.
#[tokio::test]
async fn a_many_to_one_embed_resolves_without_the_client_selecting_the_foreign_key() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/posts?select=id,author(name)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    assert_eq!(
        author_of(&body, 10).as_deref(),
        Some("alice"),
        "post 10's author must resolve without `fk_author` in the select — the join key \
         is the server's to project: {body}"
    );
    assert_eq!(author_of(&body, 11).as_deref(), Some("alice"), "post 11: {body}");
    assert_eq!(author_of(&body, 20).as_deref(), Some("bob"), "post 20: {body}");
    assert_eq!(author_of(&body, 21).as_deref(), Some("bob"), "post 21: {body}");
}

/// #1230, the other half: a key the server projected for its own use must not
/// appear in the response.
///
/// "Select it yourself" and "the server adds it and keeps it" are the same leak
/// from the client's side — the response shape would depend on which relationships
/// the schema happens to declare rather than on what was asked for.
#[tokio::test]
async fn the_join_key_the_server_projected_for_itself_is_not_returned() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/posts?select=id,author(name)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let post_10 = row_of(&body, 10);
    assert_eq!(
        keys_of(&post_10),
        vec!["author", "id"],
        "the response carries exactly what was selected; `fk_author` was projected to \
         resolve the embed and must be stripped again: {body}"
    );
}

/// The control for the strip: a client that *did* ask for the join key keeps it.
///
/// Without this, "strip `fk_author`" could be implemented as "always remove
/// `fk_author`" and both tests above would still pass.
#[tokio::test]
async fn a_client_that_selected_the_join_key_still_receives_it() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/posts?select=id,fk_author,author(name)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let post_10 = row_of(&body, 10);
    assert_eq!(
        post_10.get("fk_author").and_then(Value::as_i64),
        Some(1),
        "the client selected `fk_author`; only the server's own addition is stripped: {body}"
    );
    assert_eq!(
        author_of(&body, 10).as_deref(),
        Some("alice"),
        "and the embed still resolves: {body}"
    );
}

/// #1230's invariant: a parent whose join key is genuinely NULL stays
/// distinguishable from one whose key was merely not projected.
///
/// Post 30 has `fk_author IS NULL`. It must answer `"author": null` — the key
/// present, the value absent — while posts 10/11/20/21 in the same response carry
/// their real authors. Before the fix every row looked like post 30.
#[tokio::test]
async fn a_post_with_no_author_is_still_null_while_its_neighbours_resolve() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/posts?select=id,author(name)").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let orphan = row_of(&body, 30);
    assert_eq!(
        orphan.get("author"),
        Some(&Value::Null),
        "post 30 has no author, so `author` must be present and null: {body}"
    );
    assert_eq!(
        author_of(&body, 10).as_deref(),
        Some("alice"),
        "and a post that does have one resolves in the same response — that contrast is \
         the whole invariant: {body}"
    );
}

/// #1230 on the count path. `count_related` extracts the same join key from the
/// same parent row, so `?select=posts.count` — a document that names no flat field
/// at all — counted zero for every author.
#[tokio::test]
async fn an_embedded_count_resolves_without_the_client_selecting_the_parent_key() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=name,posts.count").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let rows = body.get("data").and_then(Value::as_array).expect("array data").clone();
    let alice = rows
        .iter()
        .find(|r| r.get("name").and_then(Value::as_str) == Some("alice"))
        .unwrap_or_else(|| panic!("alice missing: {body}"));

    assert_eq!(
        alice.get("posts_count").and_then(Value::as_i64),
        Some(2),
        "alice owns 2 posts; the count reads the parent's `id`, which this select does \
         not name: {body}"
    );
    assert_eq!(
        keys_of(alice),
        vec!["name", "posts_count"],
        "and the `id` projected to resolve the count is stripped again: {body}"
    );
}

/// The nested level has the same two halves. #864 already projects a nested
/// embed's join key into the child's sub-select — and then returns it, so
/// `posts(title,author(name))` answered with an `fk_author` the client never
/// named.
#[tokio::test]
async fn a_nested_join_key_is_projected_and_then_stripped() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(title,author(name))").await;
    assert_eq!(status, StatusCode::OK, "read should succeed: {body}");

    let posts = posts_of(&body, 1);
    assert_eq!(posts.len(), 2, "author 1 has two posts: {body}");
    for post in &posts {
        assert_eq!(
            post.get("author").and_then(|a| a.get("name")).and_then(Value::as_str),
            Some("alice"),
            "the nested ManyToOne must resolve: {body}"
        );
        assert_eq!(
            keys_of(post),
            vec!["author", "title"],
            "the nested sub-select named `title` and `author`; the join key projected to \
             resolve the embed must not survive into the response: {body}"
        );
    }
}

// ---------------------------------------------------------------------------
// #1267 — a count *inside* a sub-select
//
// #864 read a spec's sub-select for `SelectEntry::Field` and `SelectEntry::Embedded`
// and left `SelectEntry::Count` in `_ => None`, while its own comment named both. So
// `?select=id,posts(id,comments.count)` was parsed, depth-validated and relationship-
// validated, and then discarded: no `comments_count` key anywhere in the response,
// under a 200. `?select=id,posts.count` — the same count one level up — worked, so the
// depth of a selection decided silently whether it was honoured.
// ---------------------------------------------------------------------------

/// The post row with `id == post_id`, from the author row that owns it.
fn post_of(body: &Value, author_id: i64, post_id: i64) -> Value {
    posts_of(body, author_id)
        .into_iter()
        .find(|p| p.get("id").and_then(Value::as_i64) == Some(post_id))
        .unwrap_or_else(|| panic!("no post {post_id} under author {author_id}: {body}"))
}

#[tokio::test]
async fn a_nested_count_executes_at_the_validated_depth() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(id,comments.count)").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Post 10 has two comments (100, 101); post 11 has none.
    assert_eq!(
        post_of(&body, 1, 10).get("comments_count").and_then(Value::as_i64),
        Some(2),
        "post 10 owns 2 comments: {body}"
    );
    assert_eq!(
        post_of(&body, 2, 20).get("comments_count").and_then(Value::as_i64),
        Some(1),
        "post 20 owns 1 comment: {body}"
    );
}

#[tokio::test]
async fn a_nested_count_of_zero_is_reported_rather_than_omitted() {
    // The distinction the defect erased. A dropped selection and a genuine zero were
    // the same response — no key either way — so a client could not tell "this post
    // has no comments" from "the server discarded what I asked for".
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(id,comments.count)").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let post_11 = post_of(&body, 1, 11);
    assert_eq!(
        post_11.get("comments_count").and_then(Value::as_i64),
        Some(0),
        "post 11 has no comments, and must say so with a key: {body}"
    );
}

#[tokio::test]
async fn the_join_key_a_nested_count_needed_is_not_returned() {
    // `Post.comments` joins on the parent side's `id`, so a sub-select that does not
    // name `id` still needs it projected — and stripped again afterwards (#1230).
    //
    // This is the case that a `if !nested.is_empty()` gate would get wrong twice: a
    // count-only sub-select has no nested *embed*, so the count would not execute and
    // the key injected for it would never be stripped. Both halves are asserted here.
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(title,comments.count)").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let posts = posts_of(&body, 1);
    let post = posts
        .iter()
        .find(|p| p.get("title").and_then(Value::as_str) == Some("a-one"))
        .unwrap_or_else(|| panic!("post 'a-one' missing: {body}"));

    assert_eq!(
        post.get("comments_count").and_then(Value::as_i64),
        Some(2),
        "the count must execute without a sibling nested embed: {body}"
    );
    assert_eq!(
        keys_of(post),
        vec!["comments_count", "title"],
        "the parent join key the server projected for the count must not be returned: {body}"
    );
}

#[tokio::test]
async fn a_nested_count_naming_no_relationship_is_refused() {
    // Nested names are not checked at parse time — only root ones are — so this is
    // the executor's 400, raised against the *child* type. Before #1267 the entry was
    // dropped before anything could object, and the client got a 200 describing a
    // relationship the schema does not have.
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let (status, body) = rig.get("/rest/v1/authors?select=id,posts(id,bogus.count)").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.to_string().contains("bogus"),
        "the refusal must name the relationship it could not find: {body}"
    );
}
