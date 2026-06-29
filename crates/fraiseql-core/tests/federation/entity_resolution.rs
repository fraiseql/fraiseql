//! Entity resolution tests — single, batch, composite key, null handling, and
//! large-result-set resolution against **real PostgreSQL**.
//!
//! Each test provisions a real table via `common::pg_entity_fixture` and skips
//! cleanly when no Postgres is configured (the non-DB preflight leg); they run
//! for real on the Dagger `integration --suite=postgres` leg, where the
//! federation test target is invoked with a bound `DATABASE_URL`. This exercises
//! the parameterized `_entities` SQL path end-to-end (H3).

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code (skip notes to stderr)
use std::collections::HashMap;

use fraiseql_core::{
    db::{WhereClause, WhereOperator},
    federation::{
        database_resolver::DatabaseEntityResolver,
        selection_parser::FieldSelection,
        types::{EntityRepresentation, EntitySource},
    },
};
use serde_json::{Value, json};

use super::common;

/// Build a row / key map from (column, value) pairs.
fn map(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

/// Build a representation for `typename` from its key (column, value) pairs.
fn rep(typename: &str, keys: &[(&str, Value)]) -> EntityRepresentation {
    let key_fields = map(keys);
    EntityRepresentation {
        typename: typename.to_string(),
        all_fields: key_fields.clone(),
        key_fields,
    }
}

#[tokio::test]
async fn test_resolve_entity_from_postgres_table() {
    let rows = vec![map(&[
        ("id", json!("user123")),
        ("name", json!("John Doe")),
        ("email", json!("john@example.com")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text", "email text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_from_postgres_table: no postgres (set DATABASE_URL)");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let representation = rep("User", &[("id", json!("user123"))]);
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &[representation], &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (postgres table) failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity must resolve");
    assert_eq!(entity["__typename"], "User");
    assert_eq!(entity["id"], "user123");
    assert_eq!(entity["name"], "John Doe");
}

#[tokio::test]
async fn test_resolve_entities_batch_from_postgres() {
    let rows = vec![
        map(&[("id", json!("user1")), ("name", json!("Alice"))]),
        map(&[("id", json!("user2")), ("name", json!("Bob"))]),
    ];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entities_batch_from_postgres: no postgres (set DATABASE_URL)");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let reps = vec![
        rep("User", &[("id", json!("user1"))]),
        rep("User", &[("id", json!("user2"))]),
    ];
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &reps, &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (batch) failed: {e}"));

    assert_eq!(entities.len(), 2);
    // project_results preserves representation order, independent of DB row order.
    assert_eq!(entities[0].as_ref().expect("entity 0")["name"], "Alice");
    assert_eq!(entities[1].as_ref().expect("entity 1")["name"], "Bob");
}

#[tokio::test]
async fn test_resolve_entity_composite_key_from_postgres() {
    let rows = vec![map(&[
        ("tenant_id", json!("t1")),
        ("user_id", json!("u1")),
        ("name", json!("John")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["tenant_id text", "user_id text", "name text"], &rows)
            .await
    else {
        eprintln!("SKIP test_resolve_entity_composite_key_from_postgres: no postgres");
        return;
    };

    let metadata = common::metadata_composite_key("User", &["tenant_id", "user_id"]);
    let representation = rep("User", &[("tenant_id", json!("t1")), ("user_id", json!("u1"))]);
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "tenant_id".to_string(),
        "user_id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &[representation], &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (composite key) failed: {e}"));

    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].as_ref().expect("entity must resolve")["name"], "John");
}

#[tokio::test]
async fn test_resolve_entity_with_null_values_from_postgres() {
    let rows = vec![map(&[
        ("id", json!("user123")),
        ("name", json!("John")),
        ("email", Value::Null),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text", "email text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_with_null_values_from_postgres: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let representation = rep("User", &[("id", json!("user123"))]);
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &[representation], &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (null values) failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity must resolve");
    assert_eq!(entity["name"], "John");
    assert_eq!(entity["email"], Value::Null);
}

#[tokio::test]
async fn test_resolve_entity_large_result_set_from_postgres() {
    let mut rows = Vec::new();
    let mut reps = Vec::new();
    for i in 0..100 {
        let id = format!("user{i}");
        rows.push(map(&[("id", json!(id)), ("name", json!(format!("User {i}")))]));
        reps.push(rep("User", &[("id", json!(id))]));
    }
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_large_result_set_from_postgres: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &reps, &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (large result set) failed: {e}"));

    assert_eq!(entities.len(), 100);
    for entity in &entities {
        assert!(entity.is_some());
    }
}

/// C1b/R1: a per-row enforcement filter (tenant scoping) added to the key `IN`
/// clause must filter cross-tenant rows out **at the database**. Two users live in
/// separate tenants; an `_entities` request scoped to tenant A resolves A's user
/// and returns `None` for B's — proving the composed `(id IN (…)) AND
/// ("tenant_id" = $N)` SQL is valid and binds correctly against real PostgreSQL.
#[tokio::test]
async fn test_resolve_entities_enforced_filters_cross_tenant() {
    let rows = vec![
        map(&[
            ("id", json!("u-a")),
            ("name", json!("Alice")),
            ("tenant_id", json!("tenant-a")),
        ]),
        map(&[
            ("id", json!("u-b")),
            ("name", json!("Bob")),
            ("tenant_id", json!("tenant-b")),
        ]),
    ];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text", "tenant_id text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entities_enforced_filters_cross_tenant: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);
    // The caller is in tenant A; resolve both tenants' ids in one batch.
    let reps = vec![
        rep("User", &[("id", json!("u-a"))]),
        rep("User", &[("id", json!("u-b"))]),
    ];
    let tenant_filter = WhereClause::NativeField {
        column:   "tenant_id".to_string(),
        pg_cast:  String::new(),
        operator: WhereOperator::Eq,
        value:    json!("tenant-a"),
    };

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db_enforced(
            "User",
            &reps,
            &selection,
            None,
            Some(&tenant_filter),
            &[],
        )
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db_enforced failed: {e}"));

    assert_eq!(entities.len(), 2, "order preserved: one slot per representation");
    let alice = entities[0].as_ref().expect("tenant A's own user must resolve");
    assert_eq!(alice["id"], "u-a");
    assert_eq!(alice["name"], "Alice");
    assert!(
        entities[1].is_none(),
        "tenant B's user must be filtered out by the per-row tenant predicate, got: {:?}",
        entities[1]
    );
}

/// #504: a FraiseQL entity is backed by a view (`v_organization`), not a relation
/// literally named `lower(typename)` (`organization`), and its `@key` is a `uuid`.
/// Without the per-type `sql_source` map the resolver queries the non-existent
/// `organization` and errors (the gateway swallowed this into a null); with it —
/// and with the key column cast to text — the uuid-keyed row resolves.
#[tokio::test]
async fn test_resolve_view_backed_uuid_entity_uses_sql_source() {
    let org_id = "550e8400-e29b-41d4-a716-446655440000";
    let rows = vec![map(&[("id", json!(org_id)), ("name", json!("Acme"))])];
    // Backing relation `v_organization` is deliberately *not* `lower("Organization")`,
    // with a real `uuid` key column.
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("v_organization", &["id uuid", "name text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_view_backed_uuid_entity_uses_sql_source: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("Organization", "id");
    let selection = FieldSelection::new(vec!["id".to_string(), "name".to_string()]);

    // Control: without the source map the resolver guesses `lower(typename)` =
    // `organization`, which does not exist → hard error (the pre-#504 behaviour).
    let blind = DatabaseEntityResolver::new(adapter.clone(), metadata.clone());
    let blind_result = blind
        .resolve_entities_from_db(
            "Organization",
            &[rep("Organization", &[("id", json!(org_id))])],
            &selection,
        )
        .await;
    assert!(
        blind_result.is_err(),
        "querying lower(typename) must fail: relation 'organization' does not exist, got: {blind_result:?}"
    );

    // Fix: thread the entity's relation so the resolver reads `v_organization`. This
    // is the flat-column shape (no jsonb column), so the uuid key matches via the
    // `::text` cast.
    let mut sources = HashMap::new();
    sources.insert(
        "Organization".to_string(),
        EntitySource {
            relation:     "v_organization".to_string(),
            jsonb_column: None,
        },
    );
    let resolver = DatabaseEntityResolver::new(adapter, metadata).with_entity_sources(sources);
    let entities = resolver
        .resolve_entities_from_db(
            "Organization",
            &[rep("Organization", &[("id", json!(org_id))])],
            &selection,
        )
        .await
        .unwrap_or_else(|e| panic!("view-backed uuid entity resolution failed: {e}"));

    assert_eq!(entities.len(), 1);
    let org = entities[0].as_ref().expect("uuid-keyed org must resolve from its view");
    assert_eq!(org["name"], "Acme");
}

/// #504 (jsonb projection): a standard FraiseQL entity view exposes its fields in a
/// `data` jsonb column, not flat columns. With `jsonb_column = Some("data")` the
/// resolver projects each field as `data->'<snake(field)>'` (camelCase→snake
/// recasing, type-preserving) and matches the key as `data->>'id'`, so a
/// jsonb-backed entity with a camelCase, non-string field resolves. The flat-column
/// control fails because there is no bare `id` column.
#[tokio::test]
async fn test_resolve_jsonb_data_backed_entity_projects_and_recases() {
    let org_id = "550e8400-e29b-41d4-a716-446655440000";
    let rows = vec![map(&[(
        "data",
        json!({ "id": org_id, "name": "Acme", "is_customer": true }),
    )])];
    // A real jsonb-`data` view: the only column is `data`.
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("v_customer", &["data jsonb"], &rows).await
    else {
        eprintln!("SKIP test_resolve_jsonb_data_backed_entity_projects_and_recases: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("Customer", "id");
    // `isCustomer` is camelCase and jsonb-only (stored as `data->>'is_customer'`).
    let selection = FieldSelection::new(vec![
        "id".to_string(),
        "name".to_string(),
        "isCustomer".to_string(),
    ]);

    // Control: flat mode selects bare columns (`SELECT id, name, isCustomer`), which
    // do not exist on a jsonb-`data` view → hard error.
    let mut flat = HashMap::new();
    flat.insert(
        "Customer".to_string(),
        EntitySource {
            relation:     "v_customer".to_string(),
            jsonb_column: None,
        },
    );
    let flat_resolver =
        DatabaseEntityResolver::new(adapter.clone(), metadata.clone()).with_entity_sources(flat);
    let flat_result = flat_resolver
        .resolve_entities_from_db(
            "Customer",
            &[rep("Customer", &[("id", json!(org_id))])],
            &selection,
        )
        .await;
    assert!(
        flat_result.is_err(),
        "flat column SELECT must fail on a jsonb view: {flat_result:?}"
    );

    // jsonb mode: project from `data` with recasing and type fidelity.
    let mut sources = HashMap::new();
    sources.insert(
        "Customer".to_string(),
        EntitySource {
            relation:     "v_customer".to_string(),
            jsonb_column: Some("data".to_string()),
        },
    );
    let resolver = DatabaseEntityResolver::new(adapter, metadata).with_entity_sources(sources);
    let entities = resolver
        .resolve_entities_from_db(
            "Customer",
            &[rep("Customer", &[("id", json!(org_id))])],
            &selection,
        )
        .await
        .unwrap_or_else(|e| panic!("jsonb-data entity resolution failed: {e}"));

    assert_eq!(entities.len(), 1);
    let customer = entities[0].as_ref().expect("jsonb-backed customer must resolve");
    assert_eq!(customer["name"], "Acme");
    // camelCase field projected from `data->>'is_customer'`, with the boolean type
    // preserved (not the text "true") via the single-arrow `->` jsonb projection.
    assert_eq!(customer["isCustomer"], json!(true));
}
