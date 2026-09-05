//! #1275 regression: an export refuses the `?rel.field=value` filters it can never honour.
//!
//! A dotted query parameter is classified as a filter on an embedded relationship. The
//! classification loop in `RestParamExtractor::extract` `continue`s over any key containing
//! a `.` that is not a reserved parameter — so it is never a field filter and never an
//! unknown-parameter refusal — and step 9 routes it into `params.embedding_filters`.
//!
//! **That producer runs unconditionally.** It reads every query pair, whether or not
//! `?select=` named an embed, so the field arrives at the export path populated. What an
//! export cannot do is *honour* it: the only consumer is `embedding::execute_embeddings`,
//! which the export path never calls, and which — since #1268 refuses every `EmbeddedSpec`
//! on this path — could not be reached in principle. So the parameter was accepted,
//! dropped, and answered `200` with the whole unfiltered relation:
//!
//! ```text
//! GET /rest/v1/posts?author.name=alice   Accept: text/csv   ->  200, every row
//! ```
//!
//! That is accept / validate / discard, which is the shape #1268 exists to remove, left
//! behind on the same path. `bulk/mod.rs` had already settled the same parameter the other
//! way, for its own reason ("they do not contribute a WHERE clause").
//!
//! **The pair is the proof.** A request carrying a dotted parameter must be refused *and* a
//! request without one must still export. Either assertion alone is satisfied by a change
//! in the wrong direction — "refuse every export" passes the first, "refuse nothing" passes
//! the second.
//!
//! **These cases are the producer link.** The unit suite in
//! `handler::tests::export_refusal` constructs `ExtractedParams` directly and so can only
//! prove the *rule*; it cannot prove that any request a client can send ever fills the field
//! it reads. See `[[feedback_a_baseline_the_code_never_receives]]` — #1273's suite was green
//! over nine cases whose baseline no export request could carry. Every case here starts from
//! a query string.
//!
//! **Why these run without a database.** A REST read projects in Rust after the adapter
//! returns, and the refusal happens in `resolve_streaming_get_query` before any row is
//! read, so a canned `FailingAdapter` reproduces the whole path on the required `test` leg.
//! The `_pg` suite (`rest_export_embedding_e2e_pg`) measures the same requests over the wire.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use fraiseql_core::{
    db::types::JsonbValue,
    runtime::Executor,
    schema::{Cardinality, CompiledSchema, FieldType, Relationship, RestConfig},
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestFieldBuilder, TestQueryBuilder, TestTypeBuilder},
};
use serde_json::json;

use crate::routes::rest::{
    export_config::ExportConfig,
    handler::{RestError, RestHandler},
    resource::RestRouteTable,
    streaming::csv,
};

/// A streamable `posts` route whose `Post` **declares** an `author` relationship.
///
/// Declared deliberately. `extract_embedding_filters` validates no relationship name at all,
/// so a fixture with no relationships would refuse `?author.name=` and leave "the refusal is
/// really about an unknown parameter" as a live reading of every case below. With `author`
/// declared, `a_filter_naming_no_declared_relationship_is_refused_the_same_way` is the case
/// that separates the two — both are refused, and by the same branch.
fn export_schema() -> CompiledSchema {
    let mut posts = TestQueryBuilder::new("posts", "Post")
        .returns_list(true)
        .with_sql_source("v_post")
        .build();
    posts.rest_stream = true;

    let mut post = TestTypeBuilder::new("Post", "v_post")
        .with_field(TestFieldBuilder::new("title", FieldType::String).build())
        .with_field(TestFieldBuilder::new("pk_post_id", FieldType::Int).build())
        .build();
    post.relationships.push(Relationship {
        name:           "author".to_string(),
        target_type:    "Author".to_string(),
        cardinality:    Cardinality::ManyToOne,
        foreign_key:    "fk_author".to_string(),
        referenced_key: "id".to_string(),
    });

    let mut schema = CompiledSchema::new();
    schema.queries.push(posts);
    schema.types.push(post);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// BOM off so the header row is the first bytes of the body.
fn export_config() -> ExportConfig {
    ExportConfig {
        csv_include_bom: false,
        ..ExportConfig::default()
    }
}

/// Drive the real `handle_csv_get` over one canned row and return what it answered.
async fn csv_export(query_pairs: &[(&str, &str)]) -> Result<String, RestError> {
    let schema = export_schema();
    let rows = vec![JsonbValue::new(
        json!({ "pk_post_id": 1, "title": "row-1" }),
    )];
    let adapter = Arc::new(FailingAdapter::new().with_response("v_post", rows));
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let route_table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let rest_config = schema.rest_config.clone().unwrap();
    let handler = RestHandler::new(&executor, &schema, &rest_config, &route_table);

    let response = csv::handle_csv_get(
        &handler,
        &export_config(),
        "/posts",
        query_pairs,
        &HeaderMap::new(),
        None,
    )
    .await?;

    let csv::CsvBody::Stream(mut stream) = response.body;
    let mut bytes = Vec::new();
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        bytes.extend_from_slice(&chunk.expect("the CSV stream is infallible"));
    }
    Ok(String::from_utf8(bytes).expect("CSV output is UTF-8"))
}

/// The defect: a filter the export cannot apply, accepted and dropped under a `200`.
#[tokio::test]
async fn an_export_refuses_a_dotted_relationship_filter() {
    let err = match csv_export(&[("author.name", "alice")]).await {
        Ok(body) => panic!(
            "#1275: `?author.name=alice` is stored in `params.embedding_filters`, whose only \
             consumer is the JSON path's `execute_embeddings`. The export answered 200 with \
             the whole unfiltered relation and said nothing: {body:?}"
        ),
        Err(err) => err,
    };

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(
        err.message.contains("`author.name`"),
        "a client cannot act on a refusal that does not name the parameter: {}",
        err.message
    );
    assert!(
        err.message.contains("filter"),
        "the filter branch states its own diagnosis rather than the embed branch's: {}",
        err.message
    );
}

/// The bracket form fills the same field by a different producer branch.
///
/// `extract_embedding_filters` parses `rel.field[op]=value` before it parses
/// `rel.field=value`, in a separate arm with its own dot-splitting. A refusal proved only
/// against the simple form would leave the bracket form's route into the field untested.
#[tokio::test]
async fn an_export_refuses_the_bracket_form_of_a_dotted_filter() {
    let err = match csv_export(&[("author.name[eq]", "alice")]).await {
        Ok(body) => panic!("`?author.name[eq]=alice` reaches the same field: {body:?}"),
        Err(err) => err,
    };

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(
        err.message.contains("`author.name`"),
        "the field path identifies the parameter under either form: {}",
        err.message
    );
}

/// The `⚠` of #1275: no dotted key is validated against the schema before it is stored.
///
/// The classification loop routes on the dot alone, so `?nonsense.field=x` is stored under a
/// relationship `Post` does not have. It is refused by the same branch and named the same
/// way — the refusal describes what was *sent*, not something the type is known to have.
#[tokio::test]
async fn a_filter_naming_no_declared_relationship_is_refused_the_same_way() {
    let err = match csv_export(&[("nonsense.field", "x")]).await {
        Ok(body) => panic!("an undeclared dotted key is stored just as quietly: {body:?}"),
        Err(err) => err,
    };

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("`nonsense.field`"), "{}", err.message);
}

/// Every parameter is named, so a client is not left fixing them one request at a time.
///
/// Two relationships, because `embedding_filters` is keyed by relationship — one key with
/// two fields exercises the inner map, two keys exercise the outer one, and only the outer
/// one is a `HashMap` with no order of its own. The assertion is on the exact rendered list,
/// which is what makes the imposed sort a pinned contract rather than whatever iteration
/// order the run happened to produce.
#[tokio::test]
async fn every_dotted_filter_parameter_is_named_in_a_stable_order() {
    let err = csv_export(&[
        ("nonsense.field", "x"),
        ("author.name", "alice"),
        ("author.age", "40"),
    ])
    .await
    .expect_err("three dotted parameters, none of them honourable by an export");

    assert!(
        err.message.contains("`author.age`, `author.name`, `nonsense.field`"),
        "all three are named, sorted — an iteration-order message would name the same \
         request differently on consecutive runs: {}",
        err.message
    );
}

/// The half that must not move: an export with no dotted parameter still exports.
///
/// Differs from the cases above by one query pair. Without it, refusing every export would
/// satisfy all of them.
#[tokio::test]
async fn an_export_without_a_dotted_parameter_still_streams() {
    let body = csv_export(&[("title", "row-1")]).await.unwrap_or_else(|err| {
        panic!(
            "a plain field filter is not an embedding filter and must still export; \
             answered {}: {}",
            err.status, err.message
        )
    });

    assert_eq!(
        body.lines().next().unwrap_or_default(),
        "title,pk_post_id",
        "the export is the whole projection in declared field order: {body:?}"
    );
    assert!(body.contains("row-1"), "and the rows are the view's rows: {body:?}");
}

/// Order of record when a request carries both an embed and a filter for it.
///
/// The embed is reported. It is the more fundamental refusal — an export cannot embed at
/// all, which is *why* the filter has nothing to narrow — and a client told about the filter
/// first would remove it and be refused again for the `?select=`.
#[tokio::test]
async fn an_embed_is_reported_before_the_filter_on_it() {
    let err = csv_export(&[("select", "title,author(name)"), ("author.name", "alice")])
        .await
        .expect_err("both parameters are refused; only one of them is reported first");

    assert!(
        err.message.contains("embedded relationships are not available"),
        "the embed is the root refusal and is stated first: {}",
        err.message
    );
}
