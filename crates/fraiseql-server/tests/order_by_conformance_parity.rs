#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1159: over the SDK conformance fixture, compiled by the real compiler, every sorting
//! query's `orderBy.field` enum is exactly the keys the engine accepts, asked of both readers:
//! the document validator (GraphQL) and the engine itself (what REST and gRPC reach).
//!
//! No database: an accepted key reaches the failing adapter, which answers every read with
//! its own error ([`REACHED`]); a refused key is refused by name first. Any other answer
//! fails the test with its message, so a refusal for another reason can never read as an
//! acceptance.
//!
//! Both readers are asked as a caller the fixture's scoped queries admit ([`caller`]): a
//! query scoped by `inject_params` refuses a request with no caller before it reads the
//! sort key, and the fixture reads the enriched identity (a subscription policy's
//! `identity_field`), so the engine refuses a caller that never passed the enrichment
//! seam. A refusal that is not about the key says nothing about the key.

use std::sync::Arc;

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    runtime::{Executor, QueryMatch},
    schema::CompiledSchema,
    security::{ENRICHED_NAMESPACE_PREFIX, EnrichmentMark, SecurityContext},
    types::{TenantId, UserId},
};
use fraiseql_test_utils::failing_adapter::{FailError, FailingAdapter};
use serde_json::json;

/// The failing adapter's answer to every read: the only proof a key was accepted.
const REACHED: &str = "order-by parity: the query reached the database";

/// The adapter both readers reach once a key is accepted.
fn adapter() -> FailingAdapter {
    FailingAdapter::new().fail_with_error(FailError::Database {
        message:   REACHED.to_string(),
        sql_state: None,
    })
}

/// Whether `outcome` is an acceptance (the adapter's own error) or a refusal of the key
/// (one of `refusals`). Anything else fails the test, naming what `reader` answered.
fn verdict<T>(
    reader: &str,
    probe: &str,
    outcome: fraiseql_core::error::Result<T>,
    refusals: &[&str],
) -> bool {
    let Err(error) = outcome else {
        panic!("{probe} ({reader}): answered without reaching the database");
    };
    let error = error.to_string();
    if error.contains(REACHED) {
        true
    } else if refusals.iter().any(|refusal| error.contains(refusal)) {
        false
    } else {
        panic!("{probe} ({reader}): refused for a reason other than the sort key: {error}");
    }
}

/// The `fraiseql.toml` a project compiling the fixture at `path` carries, from
/// `sdks/official/conformance/project_toml.py`: the one derivation the conformance harness
/// and the generated-clients job compile it with. A copy of that rule here falls behind the
/// first time the fixture gains a construct that needs a section.
fn project_toml(path: &str) -> String {
    let script =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../sdks/official/conformance/project_toml.py");
    let output = std::process::Command::new("python3")
        .arg(script)
        .arg(path)
        .output()
        .expect("python3 runs project_toml.py");
    assert!(
        output.status.success(),
        "project_toml.py failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("project_toml.py prints UTF-8")
}

/// The fixture compiled as the conformance harness compiles it, with the project config
/// [`project_toml`] derives for it.
async fn compiled_fixture() -> CompiledSchema {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../sdks/official/conformance/reference/full.json"
    );
    let project = tempfile::TempDir::new().unwrap();
    let config = project.path().join("fraiseql.toml");
    std::fs::write(&config, project_toml(path)).unwrap();
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        config: Some(fraiseql_cli::config::ConfigSource::Explicit(config)),
        ..CompileOptions::new(path)
    })
    .await
    .expect("the conformance fixture compiles");
    CompiledSchema::from_json(&compiled.schema.to_json().unwrap(), false).expect("and loads")
}

/// A caller who holds the `tenant_id` claim the fixture's scoped queries inject, the
/// `admin` role its role-gated queries require and the `conformance` role granting its
/// scoped field, as the enrichment seam leaves a human whose identity resolved: the
/// enriched `user_id` the fixture's subscription policy reads, and the resolved mark.
fn caller() -> SecurityContext {
    let mut caller = SecurityContext {
        user_id:          UserId::from("u-conformance"),
        roles:            vec!["admin".to_string(), "conformance".to_string()],
        tenant_id:        Some(TenantId::from("A")),
        scopes:           vec![],
        attributes:       std::collections::HashMap::from([("tenant_id".to_string(), json!("A"))]),
        request_id:       "req-order-by-parity".to_string(),
        ip_address:       None,
        authenticated_at: fraiseql_core::fraiseql_time::Timestamp::now(),
        expires_at:       fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    };
    caller
        .attributes
        .insert(format!("{ENRICHED_NAMESPACE_PREFIX}user_id"), json!("u-conformance"));
    caller.mark_enrichment(EnrichmentMark::Resolved);
    caller
}

/// Whether `query` accepts `key`, by both readers, which must agree.
async fn accepts(executor: &Executor, query: &str, key: &str) -> bool {
    let caller = caller();
    let document = format!("{{ {query}(orderBy: [{{field: {key}}}]) {{ __typename }} }}");
    let probe = format!("{query}.{key}");
    let by_document = verdict(
        "document",
        &probe,
        executor.execute_with_security(&document, None, &caller).await,
        &["not one of its members", "Cannot sort by"],
    );
    let mut arguments = std::collections::HashMap::new();
    arguments.insert("orderBy".to_string(), json!([{ "field": key }]));
    let direct = QueryMatch {
        query_def: executor
            .schema()
            .queries
            .iter()
            .find(|q| q.name == query)
            .unwrap()
            .clone()
            .into(),
        fields: vec![],
        selections: std::sync::Arc::default(),
        arguments,
        operation_name: None,
        scope_where: None,
        search_relevance: None,
        parsed_query: std::sync::Arc::default(),
        collected: None,
    };
    let by_engine = verdict(
        "engine",
        &probe,
        executor.execute_query_direct(&direct, None, Some(&caller), None).await,
        &["Cannot sort by"],
    );
    assert_eq!(by_document, by_engine, "{query}.{key}: the document and the engine disagree");
    by_engine
}

#[tokio::test]
async fn the_conformance_fixture_publishes_exactly_what_it_accepts() {
    let schema = compiled_fixture().await;
    let executor = Executor::new(schema.clone(), Arc::new(adapter()));
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
