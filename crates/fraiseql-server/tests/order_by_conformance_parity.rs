#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1159: over the SDK conformance fixture, compiled by the real compiler, every sorting
//! query's `orderBy.field` enum is exactly the keys the engine accepts, asked of both readers:
//! the document validator (GraphQL) and the engine itself (what REST and gRPC reach).
//!
//! No database: an accepted key reaches the failing adapter, a refused one is refused first.

use std::sync::Arc;

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    runtime::{Executor, QueryMatch},
    schema::CompiledSchema,
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::json;

/// The fixture compiled as the conformance harness compiles it: with the project config
/// `project_toml.py` derives for it (its `[locale]` and the role granting its scope).
async fn compiled_fixture() -> CompiledSchema {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../sdks/official/conformance/reference/full.json"
    );
    let project = tempfile::TempDir::new().unwrap();
    let config = project.path().join("fraiseql.toml");
    std::fs::write(
        &config,
        "[locale]\ndefault = \"en-US\"\nallowed = [\"en-US\", \"fr-FR\"]\n\
         [[fraiseql.security.role_definitions]]\nname = \"conformance\"\n\
         scopes = [\"read:User.salary\"]\n",
    )
    .unwrap();
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        config: Some(fraiseql_cli::config::ConfigSource::Explicit(config)),
        ..CompileOptions::new(path)
    })
    .await
    .expect("the conformance fixture compiles");
    CompiledSchema::from_json(&compiled.schema.to_json().unwrap(), false).expect("and loads")
}

/// Whether `query` accepts `key`, by both readers, which must agree.
async fn accepts(executor: &Executor, query: &str, key: &str) -> bool {
    let document = format!("{{ {query}(orderBy: [{{field: {key}}}]) {{ __typename }} }}");
    let by_document = match executor.execute(&document, None).await {
        Ok(_) => true,
        Err(e) => {
            let e = e.to_string();
            !(e.contains("not one of its members") || e.contains("Cannot sort by"))
        },
    };
    let mut arguments = std::collections::HashMap::new();
    arguments.insert("orderBy".to_string(), json!([{ "field": key }]));
    let direct = QueryMatch {
        query_def: executor.schema().queries.iter().find(|q| q.name == query).unwrap().clone(),
        fields: vec![],
        selections: vec![],
        arguments,
        operation_name: None,
        scope_where: None,
        search_relevance: None,
        parsed_query: fraiseql_core::graphql::ParsedQuery::default(),
    };
    let by_engine = match executor.execute_query_direct(&direct, None, None, None).await {
        Ok(_) => true,
        Err(e) => !e.to_string().contains("Cannot sort by"),
    };
    assert_eq!(by_document, by_engine, "{query}.{key}: the document and the engine disagree");
    by_engine
}

#[tokio::test]
async fn the_conformance_fixture_publishes_exactly_what_it_accepts() {
    let schema = compiled_fixture().await;
    let executor = Executor::new(schema.clone(), Arc::new(FailingAdapter::new()));
    let mut checked = 0;
    for query in schema.queries.iter().filter(|q| !q.returns_count) {
        let Some((_, field)) =
            fraiseql_core::schema::derived_inputs::order_by_type_names(&schema, query)
        else {
            continue;
        };
        let listed: Vec<String> = schema
            .find_enum(&field)
            .unwrap_or_else(|| panic!("{}: no `{field}`", query.name))
            .values
            .iter()
            .map(|v| v.name.clone())
            .collect();
        for f in &schema.find_type(&query.return_type).unwrap().fields {
            let key = f.name.to_string();
            let accepted = accepts(&executor, &query.name, &key).await;
            assert_eq!(accepted, listed.contains(&key), "{}.{key}", query.name);
        }
        checked += 1;
    }
    assert!(checked > 0, "the fixture must exercise at least one sorting query");
}
