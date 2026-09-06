//! #1273 regression: what an export does with the six pagination parameters.
//!
//! Two rules, and both read what the client **sent** rather than the pagination the server
//! resolved: a request that put itself on a page is refused, and a `?limit=` bounds the
//! export *total* (#811). The plan is not the question, and this path discards it anyway —
//! `export_rows` removes `limit` from the query's arguments.
//!
//! `RestParamExtractor::extract` resolves every request on a `relay = true` route into a
//! `Cursor` plan, whose default arm fills `first: Some(default_page_size)` when the client
//! named no cursor at all (`resolve_pagination`). `refuse_unstreamable_request` then read
//! `params.pagination` — that plan — and matched `Cursor { .. }`, so a bare
//! `GET /rest/v1/posts` with `Accept: text/csv` answered `400 pagination not available for
//! export` naming a parameter the request never carried. A `relay = true` +
//! `rest_stream = true` query was therefore not exportable in any of the three
//! representations.
//!
//! The offset branch beside it never had the defect, and for the reason that matters here:
//! it refuses `offset > 0`, so it refuses because the client *asked to be on a page*, not
//! because the route is paginated. The two branches now read the same field —
//! `params.requested_pagination`, which records what arrived in the query string before any
//! default or clamp.
//!
//! **The pair is the proof.** A bare request must export and a request naming `?first=` must
//! still be refused; either assertion alone is satisfied by a one-line change in the wrong
//! direction (deleting the cursor branch passes the first, keeping it passes the second).
//! Every case below is the same route and the same adapter, differing only in the query
//! string.
//!
//! **Why these run without a database.** A REST read projects *in Rust*, after the adapter
//! returns, and `export_rows` reaches `stream_query_direct`, which resolves a *direct* read
//! of the view — it does not branch on `query_def.relay` the way `execute_query` does. A
//! relay route's export is therefore flat rows, exactly like any other route's, and a canned
//! `FailingAdapter` row reproduces the whole path on the required `test` leg.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use fraiseql_core::{
    db::types::JsonbValue,
    runtime::Executor,
    schema::{CompiledSchema, FieldType, RestConfig},
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

/// A streamable `posts` route, cursor- or offset-paginated.
///
/// `relay` is the whole fixture for the refusal cases: the identical schema with
/// `relay(false)` exported a bare request throughout, which is why the defect was invisible
/// to every export test written before it.
///
/// `max_page_size` is a parameter because the export total is deliberately **not** bounded
/// by it — see `an_export_total_is_not_bounded_by_max_page_size`. At the stock 1 000 no
/// fixture small enough to write by hand can tell a clamped total from an unclamped one.
fn export_schema(relay: bool, max_page_size: u64) -> CompiledSchema {
    let mut posts = TestQueryBuilder::new("posts", "Post")
        .returns_list(true)
        .relay(relay)
        .with_sql_source("v_post")
        .build();
    posts.rest_stream = true;

    let post = TestTypeBuilder::new("Post", "v_post")
        .with_field(TestFieldBuilder::new("title", FieldType::String).build())
        .with_field(TestFieldBuilder::new("pk_post_id", FieldType::Int).build())
        .build();

    let mut schema = CompiledSchema::new();
    schema.queries.push(posts);
    schema.types.push(post);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        max_page_size,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// The relay route the refusal cases use, at stock page sizes.
fn relay_export_schema() -> CompiledSchema {
    export_schema(true, RestConfig::default().max_page_size)
}

fn canned_rows(count: usize) -> Vec<JsonbValue> {
    (1..=count)
        .map(|n| JsonbValue::new(json!({ "pk_post_id": n, "title": format!("row-{n}") })))
        .collect()
}

/// BOM off so the header row is the first bytes of the body.
fn export_config() -> ExportConfig {
    ExportConfig {
        csv_include_bom: false,
        ..ExportConfig::default()
    }
}

/// Drive the real `handle_csv_get` on the relay route and return what it answered.
async fn csv_export(query_pairs: &[(&str, &str)]) -> Result<String, RestError> {
    csv_export_over(relay_export_schema(), canned_rows(1), query_pairs).await
}

/// The same, over a caller-supplied route and row set.
async fn csv_export_over(
    schema: CompiledSchema,
    rows: Vec<JsonbValue>,
    query_pairs: &[(&str, &str)],
) -> Result<String, RestError> {
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

/// The defect: no cursor parameter is sent, and the export is refused for one anyway.
#[tokio::test]
async fn a_relay_route_exports_a_request_that_named_no_cursor() {
    let body = csv_export(&[]).await.unwrap_or_else(|err| {
        panic!(
            "#1273: a bare export request on a relay route carries no cursor parameter, so \
             there is no page for the export to refuse — `resolve_pagination` fills `first` \
             with the server's default page size and the refusal used to read that. Answered \
             {}: {}",
            err.status, err.message
        )
    });

    assert_eq!(
        body.lines().next().unwrap_or_default(),
        "title,pk_post_id",
        "the export is the whole relation in declared field order — relay is a pagination \
         mode, and an export has no pages: {body:?}"
    );
    assert!(body.contains("row-1"), "and the rows are the view's rows: {body:?}");
}

/// #1278: `?first=` bounds a relay export's **total**, exactly as `?limit=` bounds an
/// offset export's.
///
/// This case used to assert the opposite, and the rule it was written against was misstated.
/// The export gate is usually described as "an export cannot be on a page", but look at what
/// it has always permitted on the offset family: `?limit=` — a **count** — bounds the export
/// total (#811), while `?offset=` — a **position** — is refused. The rule is therefore *a
/// count bounds an export; a position or a direction cannot mean anything to one*, because an
/// export starts at the beginning of the relation and there is nothing for a position to move.
///
/// `?first=` is the cursor family's count. Refusing it left a relay export bounded by nothing
/// — `?limit=` is refused on a relay route by the cross-pagination guard as the wrong
/// vocabulary, and `?first=` was refused here as a page — so the two route shapes differed in
/// **capability**, not just in spelling. `?after=`, `?before=` and `?last=` stay refused, and
/// the three cases below are what keeps that from collapsing.
///
/// 25 rows, bounded to 10: unbounded would answer 25 and a page would answer
/// `default_page_size`. Three distinguishable numbers, so neither "the bound was ignored" nor
/// "the bound was clamped to a page" can pass.
#[tokio::test]
async fn a_relay_route_export_is_bounded_by_first() {
    let body = csv_export_over(relay_export_schema(), canned_rows(25), &[("first", "10")])
        .await
        .unwrap_or_else(|err| {
            panic!(
                "`?first=10` bounds an export total on a relay route, as `?limit=10` does on an \
                 offset one. Answered {}: {}",
                err.status, err.message
            )
        });

    let data_rows = body.lines().skip(1).filter(|l| !l.trim().is_empty()).count();
    assert_eq!(
        data_rows, 10,
        "the export is bounded by the count the client sent, unclamped by max_page_size \
         (`export_rows` applies it to the stream): {body:?}"
    );
}

/// A cursor *position* is refused for the same reason, and by a different field.
///
/// `?after=` never reaches `first` at all — `resolve_pagination` leaves `first` as `None`
/// once any other cursor parameter is present — so a rule that only looked at
/// `requested.first` would export this request as though the cursor were absent.
#[tokio::test]
async fn a_relay_route_refuses_an_export_that_named_a_cursor_position() {
    let err = csv_export(&[("after", "Y3Vyc29yOjE=")])
        .await
        .expect_err("`?after=` names a position in a page sequence an export does not have");

    assert_eq!(err.status, StatusCode::BAD_REQUEST);
    assert!(
        err.message.contains("pagination not available"),
        "the refusal states its own diagnosis: {}",
        err.message
    );
}

/// Backward paging, the two parameters the forward cases cannot reach.
#[tokio::test]
async fn a_relay_route_refuses_an_export_that_paged_backwards() {
    for (name, value) in [("last", "10"), ("before", "Y3Vyc29yOjE=")] {
        let err = match csv_export(&[(name, value)]).await {
            Ok(body) => panic!("`?{name}=` must be refused by an export, got a body: {body:?}"),
            Err(err) => err,
        };

        assert_eq!(err.status, StatusCode::BAD_REQUEST, "?{name}=");
        assert!(
            err.message.contains("pagination not available"),
            "?{name}= is refused with its own diagnosis: {}",
            err.message
        );
    }
}

// ---------------------------------------------------------------------------
// `?limit=` — the one pagination parameter an export applies
// ---------------------------------------------------------------------------

/// An export total is bounded by what the client asked for, **unclamped**.
///
/// `max_page_size` bounds one page of an interactive read (#421); an export total is not a
/// page, so `export_rows` applies the client's figure to the stream instead. The resolved
/// plan has already been clamped by the time it reaches here — reading it would silently
/// truncate every export asking for more than a page, which is #811's defect.
///
/// The fixture makes the two answers differ: `max_page_size = 3` against `?limit=5` over
/// five rows. The plan says three, the request says five, and only one of them can be the
/// number of data rows in the body. The stock 1 000 is why the `_pg` suite's `?limit=307`
/// cannot see this.
#[tokio::test]
async fn an_export_total_is_not_bounded_by_max_page_size() {
    let body = csv_export_over(export_schema(false, 3), canned_rows(5), &[("limit", "5")])
        .await
        .expect("`?limit=` bounds an export total; it does not make the request unexportable");

    let data_rows = body.lines().skip(1).filter(|l| !l.is_empty()).count();
    assert_eq!(
        data_rows, 5,
        "the client asked for 5 rows and `max_page_size` is 3 — a body of 3 rows means the \
         export read the resolved plan, which is a page, instead of the request: {body:?}"
    );
}

/// The bound is applied, not merely carried: fewer rows than the table holds.
///
/// Paired with the case above, this is what stops "ignore `?limit=` entirely" from passing
/// — that would answer 5 rows there and 5 rows here.
#[tokio::test]
async fn an_export_total_below_the_row_count_truncates_the_body() {
    let body = csv_export_over(export_schema(false, 3), canned_rows(5), &[("limit", "2")])
        .await
        .expect("`?limit=2` is a total bound an export honours");

    let data_rows = body.lines().skip(1).filter(|l| !l.is_empty()).count();
    assert_eq!(data_rows, 2, "?limit=2 bounds the export to 2 rows: {body:?}");
}

/// A repeated `?limit=` bounds the export by the same occurrence every other consumer reads.
///
/// `requested_total_limit` searched the raw pairs — `.find`, first-wins — while the
/// extractor's classification assigns — last-wins, and it is the extractor's answer that
/// becomes the plan and the projection. `?limit=2&limit=4` therefore cut the export at 2
/// while the rest of the request was answered under 4: #1274's shape (one parameter, two
/// parsers, opposite resolutions) reached through a different parameter. There is one
/// reader left, so the two cannot disagree.
#[tokio::test]
async fn a_repeated_export_limit_is_bounded_by_the_occurrence_that_won() {
    let body = csv_export_over(
        export_schema(false, 10),
        canned_rows(5),
        &[("limit", "2"), ("limit", "4")],
    )
    .await
    .expect("a repeated `?limit=` is not a refusal");

    let data_rows = body.lines().skip(1).filter(|l| !l.is_empty()).count();
    assert_eq!(
        data_rows, 4,
        "the extractor resolves a repeated `?limit=` last-wins, so 4 is the figure the plan \
         and the projection were built from and must be the figure the export is cut at; \
         2 means a second parser is still reading the query string: {body:?}"
    );
}
