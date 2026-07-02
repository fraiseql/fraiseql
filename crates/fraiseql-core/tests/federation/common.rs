//! Shared fixtures and helpers for federation tests.

#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_assert_message
)] // Reason: test code, panics are acceptable
#![allow(clippy::cast_possible_truncation)] // Reason: test step counts cast usize→u32; test sizes never exceed u32::MAX
#![allow(clippy::map_unwrap_or)] // Reason: test readability preferred over method chain refactoring
use std::{collections::HashMap, sync::Arc, time::Duration};

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    federation::{
        mutation_executor::FederationMutationExecutor,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
    },
};
use serde_json::{Value, json};

/// Connect to the harness Postgres and (re)provision `table` from `column_ddl`
/// (e.g. `["id text", "amount integer"]`), seeded from `rows`. Returns the
/// connected adapter.
///
/// Returns `None` when no Postgres is configured (`DATABASE_URL` unset and no
/// local-testcontainers spawn) so the caller skips cleanly on the non-DB
/// preflight leg; the bound `Service` is returned alongside the adapter so a
/// locally-spawned container, if any, is held for the test's lifetime.
///
/// Seed values come from each row's matching column name (the first token of the
/// DDL); strings/numbers/bools render as the obvious SQL literal, missing/`Null`
/// as SQL `NULL`. Seed data is test-controlled, so it is inline-rendered and run
/// via `execute_raw_query`.
pub async fn pg_entity_fixture(
    table: &str,
    column_ddl: &[&str],
    rows: &[HashMap<String, Value>],
) -> Option<(fraiseql_test_support::Service, Arc<PostgresAdapter>)> {
    let (pg, adapter) = pg_adapter().await?;

    adapter
        .execute_raw_query(&format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#))
        .await
        .expect("drop fixture table");
    adapter
        .execute_raw_query(&format!(r#"CREATE TABLE "{table}" ({})"#, column_ddl.join(", ")))
        .await
        .expect("create fixture table");

    let col_names: Vec<&str> = column_ddl
        .iter()
        .map(|c| c.split_whitespace().next().expect("non-empty column ddl"))
        .collect();
    let col_list = col_names.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    for row in rows {
        let vals = col_names
            .iter()
            .map(|c| sql_literal(row.get(*c)))
            .collect::<Vec<_>>()
            .join(", ");
        adapter
            .execute_raw_query(&format!(r#"INSERT INTO "{table}" ({col_list}) VALUES ({vals})"#))
            .await
            .expect("seed fixture row");
    }

    Some((pg, adapter))
}

/// Connect to the harness Postgres, returning the adapter with no table
/// provisioned. `None` when no Postgres is configured (skip on the non-DB
/// preflight leg); the bound `Service` is returned so a locally-spawned
/// container, if any, is held for the test's lifetime.
pub async fn pg_adapter() -> Option<(fraiseql_test_support::Service, Arc<PostgresAdapter>)> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to harness postgres");
    Some((pg, Arc::new(adapter)))
}

/// Build a column→value map (a seed row, or a representation's key fields).
pub fn row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

/// Build an `EntityRepresentation` for `typename` from its key (column, value) pairs.
pub fn rep(typename: &str, keys: &[(&str, Value)]) -> EntityRepresentation {
    let key_fields = row(keys);
    EntityRepresentation {
        typename: typename.to_string(),
        all_fields: key_fields.clone(),
        key_fields,
    }
}

/// Render a JSON value as a SQL literal for test seed data (test-controlled).
fn sql_literal(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Null) | None => "NULL".to_string(),
        Some(other) => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

// =============================================================================
// Mutation Executor Fixture (real PostgreSQL)
// =============================================================================

/// Connect to the harness Postgres, provision each `(table, column_ddl)` as a
/// fresh empty table, and return a [`FederationMutationExecutor`] over the real
/// adapter.
///
/// `FederationMutationExecutor::execute_local_mutation` builds a plain
/// `INSERT`/`UPDATE`/`DELETE` against the lowercased entity type name and runs
/// it via `execute_raw_query`, so each test provisions exactly the columns its
/// variables reference. The table name is lowercased here to match the builder
/// (`quote_postgres_identifier(typename.to_lowercase())`), so callers can pass
/// either case without drift. `execute_extended_mutation` never touches the
/// adapter, so its tests pass an empty `tables` slice.
///
/// Returns `None` when no Postgres is configured (`DATABASE_URL` unset and no
/// local-testcontainers spawn) so the caller skips cleanly on the non-DB
/// preflight leg; the bound `Service` is returned alongside the executor so a
/// locally-spawned container, if any, is held for the test's lifetime.
pub async fn pg_mutation_executor(
    metadata: FederationMetadata,
    tables: &[(&str, &[&str])],
) -> Option<(fraiseql_test_support::Service, FederationMutationExecutor<PostgresAdapter>)> {
    let (pg, adapter) = pg_adapter().await?;

    for (table, column_ddl) in tables {
        let table = table.to_lowercase();
        adapter
            .execute_raw_query(&format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#))
            .await
            .expect("drop mutation table");
        adapter
            .execute_raw_query(&format!(r#"CREATE TABLE "{table}" ({})"#, column_ddl.join(", ")))
            .await
            .expect("create mutation table");
    }

    // These fixtures author snake_case input keys directly, so no recasing is
    // needed (recase_input_keys = false). The recasing path (camelCase surface →
    // snake_case columns) is covered by the mutation_executor unit tests.
    Some((pg, FederationMutationExecutor::new(adapter, metadata, false)))
}

// =============================================================================
// FederationMetadata Builders
// =============================================================================

/// Create a `FederationMetadata` with a single owned type with one key field.
pub fn metadata_single_key(type_name: &str, key_field: &str) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![FederatedType {
            name:                type_name.to_string(),
            keys:                vec![KeyDirective {
                fields:     vec![key_field.to_string()],
                resolvable: true,
            }],
            is_extends:          false,
            external_fields:     vec![],
            shareable_fields:    vec![],
            inaccessible_fields: vec![],
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    }
}

/// Create a `FederationMetadata` with a single extended type.
pub fn metadata_extended_type(
    type_name: &str,
    key_field: &str,
    external_fields: &[&str],
    shareable_fields: &[&str],
) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![FederatedType {
            name:                type_name.to_string(),
            keys:                vec![KeyDirective {
                fields:     vec![key_field.to_string()],
                resolvable: true,
            }],
            is_extends:          true,
            external_fields:     external_fields.iter().map(|s| (*s).to_string()).collect(),
            shareable_fields:    shareable_fields.iter().map(|s| (*s).to_string()).collect(),
            inaccessible_fields: vec![],
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    }
}

/// Create a `FederationMetadata` with a composite key.
pub fn metadata_composite_key(type_name: &str, key_fields: &[&str]) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![FederatedType {
            name:                type_name.to_string(),
            keys:                vec![KeyDirective {
                fields:     key_fields.iter().map(|s| (*s).to_string()).collect(),
                resolvable: true,
            }],
            is_extends:          false,
            external_fields:     vec![],
            shareable_fields:    vec![],
            inaccessible_fields: vec![],
            field_directives:    std::collections::HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: std::collections::HashMap::new(),
    }
}

// =============================================================================
// @requires Enforcement Helper
// =============================================================================

/// Enforce @requires directives at runtime.
///
/// Validates that all fields required by the @requires directives are present
/// in the entity representation.
pub fn enforce_requires(
    metadata: &FederationMetadata,
    typename: &str,
    fields: &[&str],
    representation: &EntityRepresentation,
) -> std::result::Result<(), String> {
    let federated_type = metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| format!("Type {} not found in federation metadata", typename))?;

    for field in fields {
        if let Some(directives) = federated_type.get_field_directives(field) {
            for required in &directives.requires {
                let field_path = required.path.join(".");
                if !representation.has_field(&field_path) {
                    return Err(format!(
                        "Validation Error: Required field missing\n\
                         Type: {}\n\
                         Field: {}\n\
                         Required: {}\n\
                         Issue: Field '{}' requires '{}' but it is missing from entity \
                         representation\n\
                         Suggestion: Ensure '{}' is requested from the owning subgraph",
                        typename, field, field_path, field, field_path, field_path
                    ));
                }
            }
        }
    }

    Ok(())
}

// =============================================================================
// Docker Network Infrastructure
// =============================================================================

pub const APOLLO_GATEWAY_URL: &str = "http://localhost:4000/graphql";
pub const USERS_SUBGRAPH_URL: &str = "http://localhost:4001/graphql";
pub const ORDERS_SUBGRAPH_URL: &str = "http://localhost:4002/graphql";
pub const PRODUCTS_SUBGRAPH_URL: &str = "http://localhost:4003/graphql";

/// Wait for a service to be ready with health check.
pub async fn wait_for_service(
    url: &str,
    max_retries: u32,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut retries = 0;

    loop {
        match client
            .post(url)
            .json(&json!({ "query": "{ __typename }" }))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                println!("✓ Service ready: {}", url);
                return Ok(());
            },
            Ok(response) => {
                println!("✗ Service {} returned status: {}", url, response.status());
            },
            Err(e) => {
                println!("✗ Service {} connection failed: {}", url, e);
            },
        }

        retries += 1;
        if retries >= max_retries {
            return Err(format!(
                "Service {} failed to become ready after {} retries",
                url, max_retries
            )
            .into());
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Execute a GraphQL query against a service.
pub async fn graphql_query(
    url: &str,
    query: &str,
) -> std::result::Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(&json!({ "query": query }))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    let body: Value = response.json().await?;
    Ok(body)
}

/// Extract data from a GraphQL response.
pub fn extract_data(response: &Value) -> Option<&Value> {
    response.get("data")
}

/// Check for GraphQL errors.
pub fn has_errors(response: &Value) -> bool {
    response.get("errors").is_some()
}

/// Get error messages from a GraphQL response.
pub fn get_error_messages(response: &Value) -> String {
    response
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|err| err.get("message")?.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_else(|| "Unknown error".to_string())
}

/// Setup test fixtures — ensures 2-subgraph services are ready.
pub async fn setup_federation_tests() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Setting up 2-subgraph federation tests ===\n");

    println!("Waiting for users subgraph...");
    wait_for_service(USERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for orders subgraph...");
    wait_for_service(ORDERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for Apollo Router gateway...");
    wait_for_service(APOLLO_GATEWAY_URL, 30).await?;

    println!("\n✓ All services ready for 2-subgraph federation tests\n");
    Ok(())
}

/// Setup helper for 3-subgraph federation tests (users -> orders -> products).
pub async fn setup_three_subgraph_tests() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Setting up 3-subgraph federation tests ===\n");

    println!("Waiting for users subgraph (port 4001)...");
    wait_for_service(USERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for orders subgraph (port 4002)...");
    wait_for_service(ORDERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for products subgraph (port 4003)...");
    wait_for_service(PRODUCTS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for Apollo Router gateway...");
    wait_for_service(APOLLO_GATEWAY_URL, 30).await?;

    println!("\n✓ All 3 subgraphs + gateway ready for federation tests\n");
    Ok(())
}
