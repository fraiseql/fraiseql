//! #1274 regression: an export's header row is the **projection**, not a second parse
//! of the raw `?select=` string.
//!
//! CSV and XLSX built their header from the request's raw `?select=` value while the
//! rows they wrote were projected from `params.field_selection`. The two disagreed
//! whenever `select` was repeated, because they resolve the repeat in opposite
//! directions:
//!
//! * `RestParamExtractor::extract` classifies with an assignment — `"select" => select_raw =
//!   Some(value)` — so the **last** occurrence wins and becomes the projection.
//! * `extract_select_columns` searched — `.find(|(k, _)| *k == "select")` — so the **first**
//!   occurrence won and became the header.
//!
//! `GET /rest/v1/posts?select=pk_post_id&select=title` therefore answered `200` with a
//! header naming `pk_post_id` over a column that was empty in every row, because
//! `write_csv_payload` renders a key the row does not carry as an empty cell —
//! indistinguishable from a column that is genuinely `NULL` throughout. That is #1268's
//! shape (a named, permanently empty column) reached by a different route.
//!
//! **Why these run without a database.** A REST read projects *in Rust*, after the
//! adapter returns: `stream_query_direct` pipes every row through `project_direct_rows`,
//! whose `ResultProjector` is built from `query_match.fields`. So a canned adapter row
//! carrying every key is still narrowed to the requested ones, and the divergence
//! reproduces exactly as it does against PostgreSQL — header and cells disagree. The
//! `_pg` suite measures the same requests end-to-end over the wire; this pair is the half
//! that runs on the **required** `test` leg.
//!
//! Each case pins the header against a request whose two `?select=` values name
//! **different** fields. A fixture repeating the same name would agree under both
//! resolutions and prove nothing — see `[[feedback_a_fixture_that_agrees_hides_the_defect]]`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::http::HeaderMap;
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
    handler::RestHandler,
    resource::RestRouteTable,
    streaming::{csv, xlsx},
};

/// A streamable `posts` route over a `Post` with two distinct scalar fields.
///
/// Two fields, not one: the whole defect is that a repeated `?select=` resolves to a
/// different field on each side, so the fixture has to be able to *tell them apart*.
///
/// `title` is declared **before** `pk_post_id` deliberately. Declared order and
/// alphabetical order would otherwise coincide, and the no-`?select=` case would read the
/// same whether the header came from the projection (declared order) or from the old
/// fallback (the first row's keys, sorted) — a fixture that agrees under both proves
/// nothing about which one ran.
fn export_schema() -> CompiledSchema {
    let mut posts = TestQueryBuilder::new("posts", "Post")
        .returns_list(true)
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
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// The one row every case exports: both fields present and distinguishable.
fn canned_rows() -> Vec<JsonbValue> {
    vec![JsonbValue::new(
        json!({ "pk_post_id": 1, "title": "hello" }),
    )]
}

/// BOM off so the header row is the first bytes of the body and comparisons are exact.
fn export_config() -> ExportConfig {
    ExportConfig {
        csv_include_bom: false,
        ..ExportConfig::default()
    }
}

/// Drive the real `handle_csv_get` and return the body it streamed.
async fn csv_body(query_pairs: &[(&str, &str)]) -> String {
    csv_body_over(query_pairs, canned_rows()).await
}

/// The same, over a caller-supplied row set — an empty one has its own header rule.
async fn csv_body_over(query_pairs: &[(&str, &str)], rows: Vec<JsonbValue>) -> String {
    let schema = export_schema();
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
    .await
    .expect("the CSV export must resolve; #1274 is about what it writes, not whether it runs");

    let csv::CsvBody::Stream(mut stream) = response.body;
    let mut bytes = Vec::new();
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        bytes.extend_from_slice(&chunk.expect("the CSV stream is infallible"));
    }
    String::from_utf8(bytes).expect("CSV output is UTF-8")
}

/// The header row of a CSV body.
fn header_of(body: &str) -> &str {
    body.lines().next().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

/// The defect, at the representation that shows it in plain text.
///
/// `select=pk_post_id&select=title` projects `title` (last-wins). The header must name
/// `title` too. Before the fix it named `pk_post_id` and the single data row was a lone
/// empty cell.
#[tokio::test]
async fn a_repeated_select_heads_the_csv_with_the_column_the_rows_were_projected_by() {
    let body = csv_body(&[("select", "pk_post_id"), ("select", "title")]).await;

    assert_eq!(
        header_of(&body),
        "title",
        "the extractor resolves a repeated `?select=` last-wins, so `title` is what was \
         projected and must be what the header names; naming `pk_post_id` puts a column \
         in the file that no row can fill: {body:?}"
    );
    assert!(
        body.contains("hello"),
        "and the value under that header must be the projected one, not an empty cell \
         indistinguishable from a genuine NULL: {body:?}"
    );
}

/// The control that makes the assertion above about the *repeat*.
///
/// Same route, same adapter, same representation — only the number of `?select=`
/// occurrences differs. A correct header here means the failure above cannot be read as
/// "CSV headers are broken".
#[tokio::test]
async fn a_single_select_still_heads_the_csv_with_what_it_named() {
    let body = csv_body(&[("select", "pk_post_id")]).await;

    assert_eq!(header_of(&body), "pk_post_id", "{body:?}");
    assert!(body.contains('1'), "the projected value is written: {body:?}");
}

/// The reverse order, so the assertion cannot pass by naming a fixed field.
///
/// `select=title&select=pk_post_id` projects `pk_post_id`. A fix that hard-coded the
/// second element, or that simply swapped `find` for `rev().find`, would pass one of
/// these two and fail the other only if both exist.
#[tokio::test]
async fn the_csv_header_follows_the_repeat_in_either_order() {
    let body = csv_body(&[("select", "title"), ("select", "pk_post_id")]).await;

    assert_eq!(header_of(&body), "pk_post_id", "{body:?}");
    assert!(body.contains('1'), "the projected value is written: {body:?}");
}

/// Without any `?select=`, the projection is the type's declared fields — so the header
/// is those fields, **in declared order**, and every one of them is fillable.
///
/// `Post` declares `title` before `pk_post_id`, so this is also the case that separates
/// the two possible sources: the projection answers `title,pk_post_id`, the old
/// first-row-keys fallback answered `pk_post_id,title`.
#[tokio::test]
async fn an_unselected_csv_export_heads_with_the_types_declared_fields() {
    let body = csv_body(&[]).await;

    assert_eq!(
        header_of(&body),
        "title,pk_post_id",
        "`RestFieldSpec::All` expands to the declared fields at the projection (#886), so \
         that is the column list the rows can fill — and in projection order, not sorted: \
         {body:?}"
    );
    assert!(body.contains("hello"), "{body:?}");
}

/// An export with no rows still names its columns.
///
/// The header-only body was previously withheld unless the client sent a `?select=`,
/// because "without a `?select=` there is no column list to write". Taking the header
/// from the projection makes that reason false — the column list is always known — so an
/// empty export is now a header and no rows, which is what a spreadsheet expects to open,
/// rather than a zero-byte file.
#[tokio::test]
async fn an_empty_csv_export_still_writes_its_header() {
    let body = csv_body_over(&[], Vec::new()).await;

    assert_eq!(
        body.trim_end(),
        "title,pk_post_id",
        "an empty export names the columns it would have carried: {body:?}"
    );
}

// ---------------------------------------------------------------------------
// XLSX — the same defect, in the copy of the parser that lived in `xlsx/mod.rs`
// ---------------------------------------------------------------------------

/// Read the header row out of a generated workbook.
///
/// `rust_xlsxwriter`'s constant-memory worksheet writes cell text inline into
/// `xl/worksheets/sheet1.xml`, so the header names are recoverable without a spreadsheet
/// reader — `zip` is the same crate and version `rust_xlsxwriter` already pulls, so this
/// adds nothing to the dependency tree.
fn xlsx_sheet_xml(bytes: &[u8]) -> String {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("an XLSX body is a ZIP archive");
    let mut sheet = archive
        .by_name("xl/worksheets/sheet1.xml")
        .expect("a workbook has a first worksheet");
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut sheet, &mut xml).expect("sheet1.xml is UTF-8");
    xml
}

/// Drive the real `handle_xlsx_get` and return its worksheet XML.
async fn xlsx_sheet(query_pairs: &[(&str, &str)]) -> String {
    let schema = export_schema();
    let adapter = Arc::new(FailingAdapter::new().with_response("v_post", canned_rows()));
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let route_table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let rest_config = schema.rest_config.clone().unwrap();
    let handler = RestHandler::new(&executor, &schema, &rest_config, &route_table);

    let response = xlsx::handle_xlsx_get(
        &handler,
        &export_config(),
        "/posts",
        query_pairs,
        &HeaderMap::new(),
        None,
    )
    .await
    .expect("the XLSX export must resolve");

    let xlsx::XlsxBody::Bytes(bytes) = response.body;
    xlsx_sheet_xml(&bytes)
}

/// The header names of a worksheet, in column order.
///
/// The first `<row>` element's inline-string cells. Order is the point: a header that
/// carries the right names in the wrong order is still a file whose columns are
/// mislabelled, and `contains` cannot see that.
fn header_cells(xml: &str) -> Vec<String> {
    let Some(row) = xml.split("<row ").nth(1).and_then(|r| r.split("</row>").next()) else {
        return Vec::new();
    };
    row.split("<t>")
        .skip(1)
        .filter_map(|c| c.split("</t>").next())
        .map(str::to_owned)
        .collect()
}

/// The XLSX half of #1274. It is a separate case because it was a separate,
/// character-for-character copy of the same parser: a fix applied to one file would have
/// left the other wrong with no compiler signal.
///
/// The workbook this produced before the fix was *worse* than the CSV. Its sheet was
/// `<dimension ref="A1"/>` with a single header cell naming `pk_post_id` and **no data
/// row at all** — `rust_xlsxwriter` omits a row whose every cell is blank, so the export
/// answered `200` with a spreadsheet containing zero records.
#[tokio::test]
async fn a_repeated_select_heads_the_workbook_with_the_projected_column() {
    let xml = xlsx_sheet(&[("select", "pk_post_id"), ("select", "title")]).await;

    assert_eq!(
        header_cells(&xml),
        vec!["title"],
        "the workbook's header must name the projected column and only it; naming \
         `pk_post_id` left a sheet with a header and no rows: {xml}"
    );
    assert!(xml.contains("hello"), "the projected value reaches the sheet: {xml}");
}

/// The XLSX control, for the same reason as the CSV one.
#[tokio::test]
async fn a_single_select_still_heads_the_workbook_with_what_it_named() {
    let xml = xlsx_sheet(&[("select", "pk_post_id")]).await;

    assert_eq!(header_cells(&xml), vec!["pk_post_id"], "{xml}");
}

/// The XLSX counterpart of the unselected CSV case, and the one that holds the XLSX
/// **wiring** rather than the shared helper.
///
/// Every other XLSX case here would still pass if `handle_xlsx_get` stopped consulting
/// the projection entirely: with a one-column selection, the old first-row-keys fallback
/// happens to produce the same single column. Only a multi-column export whose declared
/// order is not alphabetical separates the two — `Post` declares `title` before
/// `pk_post_id`, so the projection answers `[title, pk_post_id]` and the fallback answers
/// `[pk_post_id, title]`.
#[tokio::test]
async fn an_unselected_workbook_heads_with_the_types_declared_fields_in_order() {
    let xml = xlsx_sheet(&[]).await;

    assert_eq!(
        header_cells(&xml),
        vec!["title", "pk_post_id"],
        "the workbook header is the projection, in projection order — not the first \
         row's keys sorted: {xml}"
    );
    assert!(xml.contains("hello"), "{xml}");
}
