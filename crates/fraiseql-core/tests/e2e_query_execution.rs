//! Tests end-to-end query execution scenarios:
//! - Mock database with sample data
//! - Query execution with field projection
//! - ResultProjector with projected data
//! - GraphQL response envelope generation
//! - Error handling and edge cases

use std::collections::HashMap;

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::{FraiseQLError, Result},
    runtime::{FieldMapping, ProjectionMapper, ResultProjector},
    schema::SqlProjectionHint,
};
use serde_json::json;

/// Mock database adapter with sample seed data.
///
/// NOTE: This mock ignores WHERE clause arguments entirely (see
/// `execute_where_query` which discards `_where_clause`). Tests using this mock
/// cannot catch bugs in argument coercion (e.g. enum deserialization from
/// GraphQL variables, fraiseql-python issue #287). Those bugs must be caught
/// by Python-level integration tests that exercise the full SQL query path.
struct MockDatabaseAdapter {
    tables: HashMap<String, Vec<JsonbValue>>,
}

impl MockDatabaseAdapter {
    /// Create a new mock adapter with sample seed data
    fn with_sample_data() -> Self {
        let mut tables = HashMap::new();

        // Seed users table with sample data
        let users = vec![
            JsonbValue::new(json!({
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "name": "Alice Johnson",
                "email": "alice@example.com",
                "status": "active",
                "created_at": "2024-01-15T10:00:00Z",
                "updated_at": "2024-01-15T10:00:00Z",
                "metadata": {
                    "last_login": "2024-01-14T15:30:00Z",
                    "login_count": 42
                }
            })),
            JsonbValue::new(json!({
                "id": "223e4567-e89b-12d3-a456-426614174001",
                "name": "Bob Smith",
                "email": "bob@example.com",
                "status": "active",
                "created_at": "2024-01-10T09:30:00Z",
                "updated_at": "2024-01-14T11:00:00Z",
                "metadata": {
                    "last_login": "2024-01-13T20:15:00Z",
                    "login_count": 87
                }
            })),
            JsonbValue::new(json!({
                "id": "323e4567-e89b-12d3-a456-426614174002",
                "name": "Carol Davis",
                "email": "carol@example.com",
                "status": "inactive",
                "created_at": "2023-12-01T14:45:00Z",
                "updated_at": "2024-01-05T08:20:00Z",
                "metadata": {
                    "last_login": "2024-01-01T12:00:00Z",
                    "login_count": 15
                }
            })),
        ];
        tables.insert("users".to_string(), users);

        // Seed products table with sample data
        let products = vec![
            JsonbValue::new(json!({
                "id": "prod-001",
                "sku": "SKU-001",
                "name": "Product A",
                "price": 99.99,
                "stock": 150,
                "category": "Electronics",
                "available": true
            })),
            JsonbValue::new(json!({
                "id": "prod-002",
                "sku": "SKU-002",
                "name": "Product B",
                "price": 149.99,
                "stock": 75,
                "category": "Electronics",
                "available": true
            })),
            JsonbValue::new(json!({
                "id": "prod-003",
                "sku": "SKU-003",
                "name": "Product C",
                "price": 49.99,
                "stock": 0,
                "category": "Accessories",
                "available": false
            })),
        ];
        tables.insert("products".to_string(), products);

        Self { tables }
    }

    /// Create a mock adapter where nested-object fields are stored as JSON strings,
    /// matching what PostgreSQL's `->>` operator returns for JSONB nested-object columns.
    ///
    /// Use this in tests that need to exercise the projection layer's string-to-object
    /// recovery path (Fix A for issue #27).
    fn with_realistic_sample_data() -> Self {
        let mut tables = HashMap::new();

        // metadata arrives as a serialized JSON string (the ->> path)
        let users = vec![
            JsonbValue::new(json!({
                "id":         "123e4567-e89b-12d3-a456-426614174000",
                "name":       "Alice Johnson",
                "email":      "alice@example.com",
                "status":     "active",
                "created_at": "2024-01-15T10:00:00Z",
                "metadata":   r#"{"last_login":"2024-01-14T15:30:00Z","login_count":42}"#
            })),
            JsonbValue::new(json!({
                "id":         "223e4567-e89b-12d3-a456-426614174001",
                "name":       "Bob Smith",
                "email":      "bob@example.com",
                "status":     "active",
                "created_at": "2024-01-10T09:30:00Z",
                "metadata":   r#"{"last_login":"2024-01-13T20:15:00Z","login_count":87}"#
            })),
        ];
        tables.insert("users".to_string(), users);

        Self { tables }
    }

    /// Get data from a table
    fn get_table(&self, table_name: &str) -> Vec<JsonbValue> {
        self.tables.get(table_name).cloned().unwrap_or_default()
    }
}

#[async_trait]
impl DatabaseAdapter for MockDatabaseAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Fall back to standard query for tests
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        _where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        let mut results = self.get_table(view);

        if let Some(limit_val) = limit {
            results.truncate(limit_val as usize);
        }

        Ok(results)
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  5,
            active_connections: 1,
            idle_connections:   4,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

// ============================================================================
// Seed Data Tests
// ============================================================================

#[tokio::test]
async fn test_seed_data_users_available() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_seed_data_products_available() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("products", None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_seed_data_contains_correct_fields() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let users = adapter.execute_where_query("users", None, None, None).await.unwrap();

    let user = &users[0];
    let user_obj = user.as_value();
    assert_eq!(user_obj.get("name"), Some(&json!("Alice Johnson")));
    assert_eq!(user_obj.get("email"), Some(&json!("alice@example.com")));
    assert_eq!(user_obj.get("status"), Some(&json!("active")));
}

// ============================================================================
// Query Execution Tests
// ============================================================================

#[tokio::test]
async fn test_query_execution_all_users() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_query_execution_with_limit() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(2), None).await.unwrap();

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_query_execution_products() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("products", None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

// ============================================================================
// Result Projection Tests
// ============================================================================

#[tokio::test]
async fn test_result_projection_single_field() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, None, None).await.unwrap();

    // Project only id and name
    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let projected = projector.project_results(&results, true).unwrap();

    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Verify first user only has projected fields
    let first = &arr[0];
    assert!(first.get("id").is_some());
    assert!(first.get("name").is_some());
    assert_eq!(first.get("email"), None);
    assert_eq!(first.get("status"), None);
}

#[tokio::test]
async fn test_result_projection_multiple_fields() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    // Project id, name, and email
    let projector =
        ResultProjector::new(vec!["id".to_string(), "name".to_string(), "email".to_string()]);
    let projected = projector.project_results(&results, false).unwrap();

    assert!(projected.get("id").is_some());
    assert!(projected.get("name").is_some());
    assert!(projected.get("email").is_some());
    assert_eq!(projected.get("status"), None);
}

#[tokio::test]
async fn test_result_projection_products() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("products", None, None, None).await.unwrap();

    // Project only name and price
    let projector = ResultProjector::new(vec!["name".to_string(), "price".to_string()]);
    let projected = projector.project_results(&results, true).unwrap();

    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Verify each product has only projected fields
    for item in arr {
        assert!(item.get("name").is_some());
        assert!(item.get("price").is_some());
        assert_eq!(item.get("sku"), None);
        assert_eq!(item.get("category"), None);
    }
}

// ============================================================================
// GraphQL Response Tests
// ============================================================================

#[tokio::test]
async fn test_graphql_response_data_envelope() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let projected = projector.project_results(&results, false).unwrap();

    // Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(projected, "user");

    assert!(response.get("data").is_some());
    let data = response.get("data").unwrap();
    assert!(data.get("user").is_some());
}

#[tokio::test]
async fn test_graphql_response_with_typename() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let _projected = projector.project_results(&results, false).unwrap();

    // Add __typename
    let with_typename = projector.add_typename_only(&results[0], "User").unwrap();

    assert_eq!(with_typename.get("__typename"), Some(&json!("User")));
}

#[tokio::test]
async fn test_graphql_response_list_with_typename() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, None, None).await.unwrap();

    let projector = ResultProjector::new(vec!["id".to_string()]);

    // Add __typename to all results
    let with_typename = projector.add_typename_only(&results[0], "User").unwrap();

    assert_eq!(with_typename.get("__typename"), Some(&json!("User")));
}

#[tokio::test]
async fn test_graphql_error_response() {
    let error = FraiseQLError::Validation {
        message: "Invalid query field".to_string(),
        path:    Some("query.user.invalidField".to_string()),
    };

    let response = ResultProjector::wrap_error(&error);

    assert!(response.get("errors").is_some());
    assert_eq!(response.get("data"), None);

    let errors = response.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].get("message").is_some());
}

// ============================================================================
// Complete Pipeline Tests
// ============================================================================

#[tokio::test]
async fn test_complete_e2e_pipeline_single_user() {
    // Step 1: Query database
    let adapter = MockDatabaseAdapter::with_sample_data();
    let db_results = adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    assert_eq!(db_results.len(), 1);

    // Step 2: Project fields
    let projector =
        ResultProjector::new(vec!["id".to_string(), "name".to_string(), "email".to_string()]);
    let projected = projector.project_results(&db_results, false).unwrap();

    // Verify projection worked
    assert!(projected.get("id").is_some());
    assert!(projected.get("name").is_some());
    assert!(projected.get("email").is_some());
    assert_eq!(projected.get("status"), None);

    // Step 3: Add __typename
    let with_typename = projector.add_typename_only(&db_results[0], "User").unwrap();

    // Step 4: Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(with_typename, "user");

    // Verify complete response
    assert!(response.get("data").is_some());
    let data = response.get("data").unwrap();
    let user = data.get("user").unwrap();
    assert_eq!(user.get("__typename"), Some(&json!("User")));
}

#[tokio::test]
async fn test_complete_e2e_pipeline_user_list() {
    // Step 1: Query database
    let adapter = MockDatabaseAdapter::with_sample_data();
    let db_results = adapter.execute_where_query("users", None, None, None).await.unwrap();

    assert_eq!(db_results.len(), 3);

    // Step 2: Project fields
    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let projected = projector.project_results(&db_results, true).unwrap();

    // Verify projection
    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Step 3: Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(projected, "users");

    // Verify complete response
    assert!(response.get("data").is_some());
    let data = response.get("data").unwrap();
    let users = data.get("users").unwrap().as_array().unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_complete_e2e_pipeline_products() {
    // Step 1: Query database
    let adapter = MockDatabaseAdapter::with_sample_data();
    let db_results = adapter.execute_where_query("products", None, None, None).await.unwrap();

    // Step 2: Project fields
    let projector =
        ResultProjector::new(vec!["id".to_string(), "name".to_string(), "price".to_string()]);
    let projected = projector.project_results(&db_results, true).unwrap();

    let arr = projected.as_array().unwrap();
    for item in arr {
        assert!(item.get("id").is_some());
        assert!(item.get("name").is_some());
        assert!(item.get("price").is_some());
        assert_eq!(item.get("sku"), None);
    }

    // Step 3: Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(projected, "products");

    // Verify complete response
    assert!(response.get("data").is_some());
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_empty_projection_fields() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    let projector = ResultProjector::new(vec![]);
    let _projected = projector.project_results(&results, false).unwrap();

    // With empty projection fields, should still return the data object
    assert!(_projected.is_object());
}

#[tokio::test]
async fn test_projection_nonexistent_fields() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    let projector = ResultProjector::new(vec![
        "nonexistent_field".to_string(),
        "another_missing".to_string(),
    ]);
    let projected = projector.project_results(&results, false).unwrap();

    // Should return empty object for nonexistent fields
    assert!(projected.is_object());
    assert_eq!(projected.as_object().unwrap().len(), 0);
}

#[tokio::test]
async fn test_query_with_zero_limit() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(0), None).await.unwrap();

    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_large_limit() {
    let adapter = MockDatabaseAdapter::with_sample_data();
    let results = adapter.execute_where_query("users", None, Some(1000), None).await.unwrap();

    // Should return all 3 users even though we requested 1000
    assert_eq!(results.len(), 3);
}

// ============================================================================
// Data Integrity Tests
// ============================================================================

#[tokio::test]
async fn test_seed_data_not_mutated() {
    let adapter = MockDatabaseAdapter::with_sample_data();

    // Query multiple times
    let results1 = adapter.execute_where_query("users", None, None, None).await.unwrap();
    let results2 = adapter.execute_where_query("users", None, None, None).await.unwrap();

    // Should return same data
    assert_eq!(results1.len(), results2.len());
    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.as_value(), r2.as_value());
    }
}

#[tokio::test]
async fn test_different_tables_independent() {
    let adapter = MockDatabaseAdapter::with_sample_data();

    let users = adapter.execute_where_query("users", None, None, None).await.unwrap();
    let products = adapter.execute_where_query("products", None, None, None).await.unwrap();

    assert_eq!(users.len(), 3);
    assert_eq!(products.len(), 3);

    // Verify users have user fields
    assert!(users[0].as_value().get("email").is_some());

    // Verify products have product fields
    assert!(products[0].as_value().get("sku").is_some());
}

// ============================================================================
// Realistic Data Tests (nested objects as JSON strings — the ->> path)
// ============================================================================

#[tokio::test]
async fn test_projection_recovers_nested_object_from_json_string() {
    // Regression guard for issue #27.
    // When a nested-object field (metadata) arrives as a serialized JSON string
    // (the ->> path), the projection layer must return a proper object.
    let adapter = MockDatabaseAdapter::with_realistic_sample_data();
    let results = adapter.execute_where_query("users", None, None, None).await.unwrap();

    let mapper = ProjectionMapper::with_mappings(vec![
        FieldMapping::simple("id"),
        FieldMapping::simple("name"),
        FieldMapping::nested_object(
            "metadata",
            "UserMetadata",
            vec![FieldMapping::simple("last_login"), FieldMapping::simple("login_count")],
        ),
    ]);

    for row in &results {
        let result = mapper.project(row).expect("projection must not fail");

        let meta = result.get("metadata").expect("metadata field must be present");

        assert!(
            meta.is_object(),
            "metadata must be projected as an object, not a string — got: {meta}"
        );
        assert_eq!(
            meta.get("__typename"),
            Some(&json!("UserMetadata")),
            "metadata must have __typename injected"
        );
        assert!(
            meta.get("last_login").is_some(),
            "last_login must be projected from nested metadata"
        );
        assert!(
            meta.get("login_count").is_some(),
            "login_count must be projected from nested metadata"
        );
    }
}

#[tokio::test]
async fn test_e2e_pipeline_with_realistic_nested_data() {
    // Full pipeline test with realistic data:
    // DB returns metadata as a JSON string (the ->> path) →
    // projection recovers it as an object →
    // GraphQL envelope contains proper nested structure.
    let adapter = MockDatabaseAdapter::with_realistic_sample_data();
    let db_results =
        adapter.execute_where_query("users", None, Some(1), None).await.unwrap();

    let mapper = ProjectionMapper::with_mappings(vec![
        FieldMapping::simple("id"),
        FieldMapping::simple("name"),
        FieldMapping::nested_object(
            "metadata",
            "UserMetadata",
            vec![FieldMapping::simple("login_count")],
        ),
    ])
    .with_typename("User");

    let projected = mapper.project(&db_results[0]).expect("projection must not fail");
    let response = ResultProjector::wrap_in_data_envelope(projected, "user");

    let user = response["data"]["user"].as_object().expect("user must be an object");
    assert_eq!(user["__typename"], json!("User"), "top-level __typename");

    let meta = user["metadata"].as_object().expect("metadata must be an object");
    assert_eq!(
        meta["__typename"],
        json!("UserMetadata"),
        "nested __typename must be injected"
    );
    assert!(meta["login_count"].is_number(), "login_count must be a number");

    // metadata must NOT be a string anywhere in the response
    let response_str = serde_json::to_string(&response["data"]).unwrap();
    assert!(
        !response_str.contains(r#""metadata":"{\"#),
        "metadata must not appear as a JSON-encoded string in the response"
    );
}

// ============================================================================
// Double-Processing Guard (fraiseql-python issue #288)
// ============================================================================

#[tokio::test]
async fn test_projection_of_already_projected_data_is_not_empty() {
    // Regression guard for fraiseql-python issue #288.
    //
    // Verifies that the projection layer does NOT silently return empty objects
    // when fed data that has already been projected (fields at top level, no
    // nested JSONB object to extract from).
    //
    // The Python router can call the projector twice on the same data in the
    // multi-field-query path. The second call must still return meaningful data,
    // not silently strip all fields.
    let already_projected = JsonbValue::new(serde_json::json!({
        "__typename": "MyItem",
        "id":         "item-1",
        "nested":     { "__typename": "Nested", "value": "hello" }
    }));

    let mapper = ProjectionMapper::with_mappings(vec![
        FieldMapping::simple("id"),
        FieldMapping::nested_object("nested", "Nested", vec![FieldMapping::simple("value")]),
    ])
    .with_typename("MyItem");

    let result = mapper.project(&already_projected).unwrap();
    let obj = result.as_object().expect("result must be an object");

    assert_eq!(
        obj.get("__typename"),
        Some(&serde_json::json!("MyItem")),
        "top-level __typename must be present"
    );
    assert!(obj.get("id").is_some(), "id must be present — got empty object: {result}");

    let nested = obj.get("nested").expect("nested must be present");
    assert!(nested.is_object(), "nested must be an object, not absent or string");
    assert!(
        nested.get("value").is_some(),
        "nested.value must be present — silent empty: {nested}"
    );
}
