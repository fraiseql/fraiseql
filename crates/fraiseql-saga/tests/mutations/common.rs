//! Shared fixtures for the saga mutation tests.
//!
//! Moved from `fraiseql-core`'s federation test common when the saga moved above
//! the engine (#1354). `metadata_single_key` / `metadata_composite_key` still
//! exist there for the entity tests; only the mutation fixtures live here.

#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test code, panics are acceptable
use std::sync::Arc;

use fraiseql_db::{postgres::PostgresAdapter, traits::DatabaseAdapter};
use fraiseql_federation::types::{FederatedType, FederationMetadata, KeyDirective};
use fraiseql_saga::mutation_executor::FederationMutationExecutor;

/// Connect to the harness Postgres, returning the adapter with no table
/// provisioned. `None` when no Postgres is configured (skip on the non-DB
/// preflight leg); the bound `Service` is returned so a locally-spawned
/// container, if any, is held for the test's lifetime.
pub async fn pg_adapter() -> Option<(fraiseql_test_support::Service, Arc<PostgresAdapter>)> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to harness postgres");
    Some((pg, Arc::new(adapter)))
}

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
