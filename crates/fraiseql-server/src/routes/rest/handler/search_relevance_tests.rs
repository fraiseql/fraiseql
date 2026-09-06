//! #1284: what a `?search=` request asks the executor to order by.
//!
//! `?search=` **without** `?sort=` answered `400` on every REST representation.
//! The handler wrote `arguments["orderBy"] = [{"_relevance": "desc"}]` — an array
//! whose item has no `field` key, which `OrderByClause::from_graphql_json`
//! refuses — and nothing anywhere in the workspace consumed a `_relevance` key,
//! so even a parseable spelling would have sorted on `data->>'_relevance'`: NULL
//! on every row, no ordering at all, under a `200`.
//!
//! So the documented default path of full-text search — the OpenAPI description
//! this server generates promises "Results are ranked by relevance unless `sort`
//! is specified" — was the one that could not succeed.
//!
//! These cases pin the request the handler builds; they are the half the required
//! `test` leg can run. That the SQL it becomes actually ranks rows needs a
//! database: `tests/rest_search_relevance_e2e_pg.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::http::HeaderMap;
use fraiseql_core::{
    runtime::Executor,
    schema::{CompiledSchema, FieldType, RestConfig},
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestFieldBuilder, TestQueryBuilder, TestTypeBuilder},
};

use crate::routes::rest::{
    handler::{RestHandler, routing::ResolvedGetQuery},
    resource::RestRouteTable,
};

/// A `docs` route over a type with two searchable (String) fields and one that
/// is not.
///
/// `bodyText` is deliberately multi-word: the rank has to extract
/// `data->>'body_text'`, the storage key the predicate is lowered to, and a
/// `camelCase` key would rank a column that does not exist — NULL on every row,
/// which is the silent half of this defect.
fn docs_schema() -> CompiledSchema {
    let docs = TestQueryBuilder::new("docs", "Doc")
        .returns_list(true)
        .with_sql_source("v_doc")
        .build();

    let doc = TestTypeBuilder::new("Doc", "v_doc")
        .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
        .with_field(TestFieldBuilder::new("title", FieldType::String).build())
        .with_field(TestFieldBuilder::new("bodyText", FieldType::String).build())
        .build();

    let mut schema = CompiledSchema::new();
    schema.queries.push(docs);
    schema.types.push(doc);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// Resolve a GET against the `docs` route and hand back what the executor would
/// receive.
fn resolve(query_pairs: &[(&str, &str)]) -> ResolvedGetQuery {
    let schema = docs_schema();
    let adapter = Arc::new(FailingAdapter::new());
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let route_table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let rest_config = schema.rest_config.clone().unwrap();
    let handler = RestHandler::new(&executor, &schema, &rest_config, &route_table);
    handler
        .resolve_get_query("/docs", query_pairs, &HeaderMap::new())
        .expect("the documented default path of `?search=` must resolve")
}

/// The defect, stated as the request: a search with no sort carries a relevance
/// ordering, and `arguments` carries **no** `orderBy` at all.
///
/// Both halves are the fix. The ordering has to exist (or nothing ranks), and it
/// has to be somewhere other than the argument map (or it is `_relevance` again
/// — a string in a JSON value that no type constrains, which is why the failure
/// surfaced three layers below the line that wrote it).
#[test]
fn a_search_with_no_sort_carries_a_relevance_ordering_outside_the_arguments() {
    let resolved = resolve(&[("search", "ada lovelace")]);

    let relevance = resolved
        .query_match
        .search_relevance
        .as_ref()
        .expect("`?search=` with no `?sort=` is ranked by relevance");
    assert_eq!(relevance.query, "ada lovelace");
    assert_eq!(
        relevance.fields,
        vec!["title".to_string(), "body_text".to_string()],
        "the rank covers the searchable fields, as the storage keys the predicate uses"
    );

    assert!(
        !resolved.query_match.arguments.contains_key("orderBy"),
        "the ranking is not a client argument: {:?}",
        resolved.query_match.arguments
    );
}

/// The search still narrows the rows — the ordering is the half that changed.
#[test]
fn a_search_still_builds_its_predicate() {
    let resolved = resolve(&[("search", "ada")]);
    let where_arg = resolved.query_match.arguments.get("where").expect("`?search=` filters");
    let or = where_arg["_or"].as_array().expect("two searchable fields are OR-ed");
    assert_eq!(or.len(), 2, "{where_arg}");
    assert_eq!(or[0]["title"]["websearch_query"], "ada");
}

/// A client that named a sort gets that sort, and no ranking — which is exactly
/// what the generated OpenAPI document says ("ranked by relevance **unless**
/// `sort` is specified").
///
/// This is the case that already worked before the fix, and the one a repair
/// could most easily break by ranking unconditionally.
#[test]
fn an_explicit_sort_wins_and_leaves_no_ranking() {
    let resolved = resolve(&[("search", "ada"), ("sort", "id")]);

    assert!(
        resolved.query_match.search_relevance.is_none(),
        "the client named a sort; nothing should override it"
    );
    let order_by = resolved.query_match.arguments.get("orderBy").expect("the client's sort");
    assert_eq!(order_by[0]["field"], "id");
}

/// No search, no ranking — the control that keeps the first case from passing
/// against a handler that ranks every read.
#[test]
fn a_read_without_a_search_carries_no_ranking() {
    let resolved = resolve(&[]);
    assert!(resolved.query_match.search_relevance.is_none());
    assert!(!resolved.query_match.arguments.contains_key("orderBy"));
}
