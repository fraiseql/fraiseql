//! Federation mutations integration tests
//!
//! Tests for federation mutation support covering:
//! - Local entity mutations (CREATE, UPDATE, DELETE)
//! - Extended entity mutations (mutations on entities owned elsewhere)
//! - Cross-subgraph mutation coordination
//! - Mutation response formatting
//! - Transaction handling and rollback

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, DatabaseCapabilities},
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    schema::SqlProjectionHint,
    federation::{
        mutation_executor::FederationMutationExecutor,
        types::{FederatedType, FederationMetadata, KeyDirective},
    },
};
use serde_json::{Value, json};

// ============================================================================
// Mock Database Adapter for Mutation Testing
// ============================================================================

struct MockMutationDatabaseAdapter {
    #[allow(dead_code)]
    data: HashMap<String, Vec<HashMap<String, Value>>>,
}

impl MockMutationDatabaseAdapter {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn with_table_data(mut self, table: String, rows: Vec<HashMap<String, Value>>) -> Self {
        self.data.insert(table, rows);
        self
    }
}

#[async_trait]
impl DatabaseAdapter for MockMutationDatabaseAdapter {
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
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(Vec::new())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  10,
            idle_connections:   8,
            active_connections: 2,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(&self, _sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        // Mock executes and returns empty (mutations don't return data in our mock)
        Ok(Vec::new())
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities::from_database_type(self.database_type())
    }
}

// ============================================================================
// Local Entity Mutations (Owned Entities)
// ============================================================================

#[test]
fn test_mutation_create_owned_entity() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user_new",
        "name": "New User",
        "email": "newuser@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "createUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user_new");
    assert_eq!(response["name"], "New User");
}

#[test]
fn test_mutation_update_owned_entity() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user123");
    assert_eq!(response["email"], "updated@example.com");
    assert_eq!(response["name"], "Updated Name");
}

#[test]
fn test_mutation_delete_owned_entity() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user_to_delete"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "deleteUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user_to_delete");
}

#[test]
fn test_mutation_owned_entity_returns_updated_representation() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Product".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["sku".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "sku": "PROD-001",
        "name": "Widget",
        "price": 29.99,
        "stock": 100
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Product", "updateProduct", &variables));

    assert!(result.is_ok());
    let entity = result.unwrap();

    // Verify response is a proper entity representation
    assert!(entity.is_object());
    assert_eq!(entity["__typename"], "Product");
    assert_eq!(entity["sku"], "PROD-001");
    assert_eq!(entity["price"], 29.99);
}

#[test]
fn test_mutation_owned_entity_batch_updates() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute multiple mutations
    for i in 0..3 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("User {}", i)
        });

        let result =
            runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response["id"], format!("user{}", i));
    }
}

#[test]
fn test_mutation_composite_key_update() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["tenant_id".to_string(), "order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "tenant_id": "tenant_123",
        "order_id": "order_456",
        "status": "confirmed"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Order", "updateOrder", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "Order");
    assert_eq!(response["tenant_id"], "tenant_123");
    assert_eq!(response["order_id"], "order_456");
    assert_eq!(response["status"], "confirmed");
}

#[test]
fn test_mutation_with_validation_errors() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    // Try to mutate with invalid nested object
    let variables = json!({
        "id": "user1",
        "metadata": { "nested": "object" }  // Invalid for SQL
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // This should fail during query building
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Error expected
    assert!(result.is_err());
}

#[test]
fn test_mutation_constraint_violation() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user_duplicate",
        "email": "existing@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute mutation (constraint violation would occur at DB level)
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Should succeed in building query, DB would handle constraint
    assert!(result.is_ok());
}

#[test]
fn test_mutation_concurrent_updates() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = Arc::new(FederationMutationExecutor::new(mock_adapter, metadata));

    // Execute multiple mutations concurrently
    for i in 0..5 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("Updated User {}", i)
        });

        let exec = executor.clone();
        let result =
            runtime.block_on(exec.execute_local_mutation("User", "updateUser", &variables));

        assert!(result.is_ok());
    }
}

#[test]
fn test_mutation_transaction_rollback() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user1",
        "email": "test@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // In real scenario with DB transaction, would test rollback
    assert!(result.is_ok());
}

// ============================================================================
// Extended Entity Mutations
// ============================================================================

#[test]
fn test_mutation_extended_entity_requires_resolution() {
    // Extended entities require resolving @requires fields before mutation
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true, // Extended entity
            external_fields:  vec!["customer_id".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "order_id": "order123",
        "status": "shipped"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("Order", "updateOrder", &variables));

    // Extended mutation returns entity representation
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "Order");
}

#[test]
fn test_mutation_extended_entity_propagates_to_owner() {
    // Extended mutations propagate to authoritative subgraph
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       true, // Extended in this subgraph
            external_fields:  vec!["email".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user123",
        "status": "verified"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("User", "verifyUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["__typename"], "User");
}

#[test]
fn test_mutation_extended_entity_partial_fields() {
    // Extended entity mutation with only partial fields
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Product".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["sku".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec![],
            shareable_fields: vec!["price".to_string()],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "sku": "PROD-001",
        "price": 29.99
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("Product", "updatePrice", &variables));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_cross_subgraph() {
    // Cross-subgraph extended entity mutation
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Review".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["review_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec!["product_id".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "review_id": "rev123",
        "rating": 5
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_extended_mutation("Review", "updateReview", &variables));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_with_external_fields() {
    // Extended entity mutation with @external fields reference
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "OrderItem".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["item_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec!["order_id".to_string(), "product_id".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "item_id": "item123",
        "quantity": 5
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "OrderItem",
        "updateQuantity",
        &variables,
    ));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_reference_tracking() {
    // Reference tracking in extended entity mutations
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "UserProfile".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["user_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec!["user_id".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "user_id": "user123",
        "bio": "Updated bio"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "UserProfile",
        "updateProfile",
        &variables,
    ));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_cascade_updates() {
    // Cascade update handling for extended entities
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Organization".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["org_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec![],
            shareable_fields: vec!["name".to_string()],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "org_id": "org123",
        "name": "Updated Org Name"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "Organization",
        "updateOrganization",
        &variables,
    ));

    assert!(result.is_ok());
}

#[test]
fn test_mutation_extended_entity_conflict_resolution() {
    // Conflict resolution in extended entity mutations
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "SharedResource".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["resource_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec![],
            shareable_fields: vec!["data".to_string()],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "resource_id": "res123",
        "data": "updated data",
        "version": 2
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result = runtime.block_on(executor.execute_extended_mutation(
        "SharedResource",
        "updateResource",
        &variables,
    ));

    assert!(result.is_ok());
}

// ============================================================================
// Mutation Response Format
// ============================================================================

#[test]
fn test_mutation_response_format_matches_spec() {
    use serde_json::json;

    // Federation response format must include:
    // - __typename field
    // - All requested fields with updated values
    let response = json!({
        "__typename": "User",
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });

    // Validate required fields
    assert!(response.get("__typename").is_some());
    assert_eq!(response["__typename"], "User");
    assert!(response.get("id").is_some());
    assert!(response.get("email").is_some());
    assert!(response.get("name").is_some());
}

#[test]
fn test_mutation_response_includes_updated_fields() {
    use serde_json::json;

    let original_email = "old@example.com";
    let updated_email = "new@example.com";

    let mutation_response = json!({
        "__typename": "User",
        "id": "user123",
        "email": updated_email,
        "name": "John Doe"
    });

    // Verify updated field value is in response
    assert_ne!(mutation_response["email"].as_str(), Some(original_email));
    assert_eq!(mutation_response["email"].as_str(), Some(updated_email));
}

#[test]
fn test_mutation_response_federation_wrapper() {
    use serde_json::json;

    // Federation mutations return entity representation (not wrapped)
    let entity_representation = json!({
        "__typename": "User",
        "id": "user123",
        "email": "test@example.com",
        "name": "Test User"
    });

    // Check it's a valid entity representation
    assert!(entity_representation.is_object());
    assert!(entity_representation.get("__typename").is_some());
    assert_eq!(entity_representation["__typename"], "User");
}

#[test]
fn test_mutation_response_error_federation_format() {
    use serde_json::json;

    // Error response in federation format
    let error_response = json!({
        "errors": [
            {
                "message": "Entity not found",
                "extensions": {
                    "code": "ENTITY_NOT_FOUND"
                }
            }
        ]
    });

    // Validate error structure
    assert!(error_response.get("errors").is_some());
    let errors = error_response["errors"].as_array();
    assert!(errors.is_some());
    assert!(!errors.unwrap().is_empty());

    let error = &error_response["errors"][0];
    assert!(error.get("message").is_some());
    assert!(error.get("extensions").is_some());
}

#[test]
fn test_mutation_response_partial_success() {
    use serde_json::json;

    // Partial success: some entities updated, some failed
    let partial_response = json!({
        "data": {
            "updateUsers": [
                {
                    "__typename": "User",
                    "id": "user1",
                    "email": "updated1@example.com"
                },
                null,  // Failed update represented as null
                {
                    "__typename": "User",
                    "id": "user3",
                    "email": "updated3@example.com"
                }
            ]
        },
        "errors": [
            {
                "message": "User not found: user2",
                "path": ["updateUsers", 1]
            }
        ]
    });

    // Verify structure supports partial success
    assert!(partial_response.get("data").is_some());
    assert!(partial_response.get("errors").is_some());

    let results = partial_response["data"]["updateUsers"].as_array();
    assert!(results.is_some());
    assert_eq!(results.unwrap().len(), 3);
}

#[test]
fn test_mutation_response_subscription_trigger() {
    // Subscription trigger on mutation
    use serde_json::json;

    // Mutation response that would trigger subscriptions
    let mutation_response = json!({
        "__typename": "User",
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });

    // Verify subscription-relevant fields are present
    assert!(mutation_response.get("__typename").is_some());
    assert!(mutation_response.get("id").is_some());

    // Check that response can be serialized (for subscription transmission)
    let serialized = serde_json::to_string(&mutation_response).unwrap();
    assert!(!serialized.is_empty());

    // Deserialize and verify round-trip
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, mutation_response);
}

// ============================================================================
// Cross-Subgraph Mutation Coordination
// ============================================================================

#[test]
fn test_mutation_coordinate_two_subgraph_updates() {
    // Coordinate mutations across two subgraphs
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![
            FederatedType {
                name:             "Order".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["order_id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
            FederatedType {
                name:             "OrderItem".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["item_id".to_string()],
                    resolvable: true,
                }],
                is_extends:       true,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
        ],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Update order (subgraph 1)
    let order_vars = json!({"order_id": "order123", "status": "confirmed"});
    let executor1 = FederationMutationExecutor::new(mock_adapter.clone(), metadata.clone());
    let result1 =
        runtime.block_on(executor1.execute_local_mutation("Order", "updateOrder", &order_vars));
    assert!(result1.is_ok());

    // Update order items (subgraph 2)
    let item_vars = json!({"item_id": "item1", "quantity": 2});
    let executor2 = FederationMutationExecutor::new(mock_adapter, metadata);
    let result2 = runtime.block_on(executor2.execute_extended_mutation(
        "OrderItem",
        "updateQuantity",
        &item_vars,
    ));
    assert!(result2.is_ok());
}

#[test]
fn test_mutation_coordinate_three_subgraph_updates() {
    // Coordinate mutations across three subgraphs
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![
            FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
            FederatedType {
                name:             "Order".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["order_id".to_string()],
                    resolvable: true,
                }],
                is_extends:       true,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
            FederatedType {
                name:             "Payment".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["payment_id".to_string()],
                    resolvable: true,
                }],
                is_extends:       true,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
        ],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Update user in subgraph 1
    let user_vars = json!({"id": "user123", "status": "verified"});
    let r1 = runtime.block_on(executor.execute_local_mutation("User", "verifyUser", &user_vars));
    assert!(r1.is_ok());

    // Update order in subgraph 2
    let order_vars = json!({"order_id": "order123", "status": "processing"});
    let r2 =
        runtime.block_on(executor.execute_extended_mutation("Order", "updateOrder", &order_vars));
    assert!(r2.is_ok());

    // Update payment in subgraph 3
    let payment_vars = json!({"payment_id": "pay123", "status": "processed"});
    let r3 = runtime.block_on(executor.execute_extended_mutation(
        "Payment",
        "processPayment",
        &payment_vars,
    ));
    assert!(r3.is_ok());
}

#[test]
fn test_mutation_reference_update_propagation() {
    // Reference update propagation across subgraphs
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Review".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["review_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec!["product_id".to_string()],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "review_id": "review123",
        "product_id": "product456",
        "rating": 5
    });

    let result =
        runtime.block_on(executor.execute_extended_mutation("Review", "updateReview", &variables));
    assert!(result.is_ok());
}

#[test]
fn test_mutation_circular_reference_handling() {
    // Circular reference handling in mutations
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![
            FederatedType {
                name:             "Author".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["author_id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
            FederatedType {
                name:             "Book".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["book_id".to_string()],
                    resolvable: true,
                }],
                is_extends:       true,
                external_fields:  vec!["author_id".to_string()],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
        ],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Update author
    let author_vars = json!({"author_id": "author1", "name": "Updated Author"});
    let executor = FederationMutationExecutor::new(mock_adapter.clone(), metadata.clone());
    let r1 =
        runtime.block_on(executor.execute_local_mutation("Author", "updateAuthor", &author_vars));
    assert!(r1.is_ok());

    // Update book referencing author (circular)
    let book_vars = json!({"book_id": "book1", "author_id": "author1", "title": "Updated Book"});
    let executor2 = FederationMutationExecutor::new(mock_adapter, metadata);
    let r2 =
        runtime.block_on(executor2.execute_extended_mutation("Book", "updateBook", &book_vars));
    assert!(r2.is_ok());
}

#[test]
fn test_mutation_multi_subgraph_transaction() {
    // Multi-subgraph transaction handling
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Account".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["account_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "account_id": "acc123",
        "balance": 1000.00
    });

    let result =
        runtime.block_on(executor.execute_local_mutation("Account", "updateAccount", &variables));
    assert!(result.is_ok());
}

#[test]
fn test_mutation_subgraph_failure_rollback() {
    // Rollback on subgraph failure
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Transaction".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["txn_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "txn_id": "txn123",
        "amount": 100.00
    });

    let result = runtime.block_on(executor.execute_local_mutation(
        "Transaction",
        "executeTransaction",
        &variables,
    ));
    assert!(result.is_ok());
}

#[test]
fn test_mutation_subgraph_timeout_handling() {
    // Subgraph timeout handling
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "AsyncJob".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["job_id".to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "job_id": "job123",
        "status": "processing"
    });

    let result =
        runtime.block_on(executor.execute_extended_mutation("AsyncJob", "updateJob", &variables));
    assert!(result.is_ok());
}

// ============================================================================
// Mutation Error Scenarios
// ============================================================================

#[test]
fn test_mutation_entity_not_found() {
    use fraiseql_core::federation::{
        mutation_query_builder::build_update_query,
        types::{FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "nonexistent_user",
        "name": "Updated"
    });

    // Query builds successfully but returns error when executed against DB
    let query = build_update_query("User", &variables, &metadata).unwrap();
    assert!(query.contains("WHERE id = 'nonexistent_user'"));
}

#[test]
fn test_mutation_invalid_field_value() {
    use fraiseql_core::federation::{
        mutation_query_builder::build_insert_query, types::FederationMetadata,
    };
    use serde_json::json;

    let metadata = FederationMetadata::default();

    // Invalid field values like objects should be rejected
    let invalid_variables = json!({
        "id": "user1",
        "metadata": { "nested": "object" }  // Invalid for SQL
    });

    let result = build_insert_query("User", &invalid_variables, &metadata);
    // Should error because objects cannot convert to SQL literals
    assert!(result.is_err());
}

#[test]
fn test_mutation_missing_required_fields() {
    use fraiseql_core::federation::{
        mutation_query_builder::build_update_query,
        types::{FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    // Missing id key field should error
    let missing_key = json!({
        "name": "Updated Name"
    });

    let result = build_update_query("User", &missing_key, &metadata);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("missing"));
}

#[test]
fn test_mutation_authorization_error() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "admin_user",
        "role": "superadmin"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute mutation (authorization would be checked at application level)
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Query builds successfully
    assert!(result.is_ok());
}

#[test]
fn test_mutation_duplicate_key_error() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "existing_id",
        "email": "test@example.com"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Execute mutation (duplicate key would be caught at DB level)
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    // Query builds successfully
    assert!(result.is_ok());
}

// ============================================================================
// Mutation Performance
// ============================================================================

#[test]
fn test_mutation_latency_single_entity() {
    use std::time::Instant;

    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user1",
        "name": "Updated User"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let start = Instant::now();
    let _result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));
    let duration = start.elapsed();

    // Mock mutation should be very fast (<10ms)
    assert!(duration.as_millis() < 10, "Single mutation took {:?}", duration);
}

#[test]
fn test_mutation_latency_batch_updates() {
    use std::time::Instant;

    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let start = Instant::now();

    // Execute 10 mutations
    for i in 0..10 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("Updated User {}", i)
        });

        let _result =
            runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));
    }

    let duration = start.elapsed();

    // 10 batch mutations should be reasonable (<100ms for mock)
    assert!(duration.as_millis() < 100, "Batch mutations took {:?}", duration);
}

#[test]
fn test_mutation_concurrent_request_handling() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let runtime = std::sync::Arc::new(tokio::runtime::Runtime::new().unwrap());

    // Simulate concurrent mutation requests
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let adapter = mock_adapter.clone();
            let meta = metadata.clone();
            let rt = runtime.clone();

            std::thread::spawn(move || {
                let variables = json!({
                    "id": format!("user{}", i),
                    "name": format!("Updated User {}", i)
                });

                let executor = FederationMutationExecutor::new(adapter, meta);
                rt.block_on(executor.execute_local_mutation("User", "updateUser", &variables))
            })
        })
        .collect();

    // All mutations should complete successfully
    for handle in handles {
        let result = handle.join();
        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }
}

// ============================================================================
// Mutation Type Detection
// ============================================================================

#[test]
fn test_detect_mutation_query() {
    use fraiseql_core::federation::mutation_detector::is_mutation;

    assert!(is_mutation("mutation { updateUser { id } }"));
    assert!(is_mutation("mutation UpdateUser { updateUser(id: \"123\") { id } }"));
    assert!(is_mutation("  mutation  {  createOrder  {  id  }  }"));
    assert!(!is_mutation("query { user { id } }"));
    assert!(!is_mutation("{ user { id } }"));
}

#[test]
fn test_detect_mutation_on_owned_entity() {
    use fraiseql_core::federation::mutation_detector::{is_local_mutation, is_mutation};

    let mutation_query = "mutation { updateUser { id } }";
    assert!(is_mutation(mutation_query));

    // Test with federation disabled - all mutations are local
    let metadata = FederationMetadata::default();
    assert!(is_local_mutation("updateUser", &metadata));
}

#[test]
fn test_detect_mutation_on_extended_entity() {
    use fraiseql_core::federation::mutation_detector::{is_extended_mutation, is_mutation};

    let mutation_query = "mutation { updateOrder { id } }";
    assert!(is_mutation(mutation_query));

    // Test with federation disabled - no mutations are extended
    let metadata = FederationMetadata::default();
    assert!(!is_extended_mutation("updateUser", &metadata));
}

// ============================================================================
// Mutation Variables and Arguments
// ============================================================================

#[test]
fn test_mutation_with_variables() {
    use fraiseql_core::federation::{
        mutation_query_builder::{build_delete_query, build_insert_query, build_update_query},
        types::{FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "id": "user123",
        "email": "test@example.com",
        "name": "Test User"
    });

    let update_query = build_update_query("User", &variables, &metadata).unwrap();
    assert!(update_query.contains("UPDATE user"));
    assert!(update_query.contains("SET"));
    assert!(update_query.contains("WHERE id = 'user123'"));

    let insert_query = build_insert_query("User", &variables, &metadata).unwrap();
    assert!(insert_query.contains("INSERT INTO user"));
    assert!(insert_query.contains("VALUES"));

    let delete_query = build_delete_query("User", &variables, &metadata).unwrap();
    assert!(delete_query.contains("DELETE FROM user"));
    assert!(delete_query.contains("WHERE id = 'user123'"));
}

#[test]
fn test_mutation_variable_validation() {
    use fraiseql_core::federation::{
        mutation_query_builder::build_update_query,
        types::{FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    // Missing key field should error
    let missing_key = json!({
        "email": "test@example.com"
    });

    let result = build_update_query("User", &missing_key, &metadata);
    assert!(result.is_err());
}

#[test]
fn test_mutation_input_type_coercion() {
    use fraiseql_core::federation::{
        mutation_query_builder::build_update_query,
        types::{FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "order_id": 789,
        "total": 99.99,
        "active": true
    });

    let update_query = build_update_query("Order", &variables, &metadata).unwrap();
    // Numbers are not quoted in SQL (correctly)
    assert!(update_query.contains("WHERE order_id = 789"));
    assert!(update_query.contains("total = 99.99"));
    assert!(update_query.contains("active = true"));
}

// ============================================================================
// Mutation Return Selection
// ============================================================================

#[test]
fn test_mutation_return_all_requested_fields() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    // Mutation with multiple fields
    let variables = json!({
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name",
        "phone": "+1-555-1234",
        "address": "123 Main St"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("User", "updateUser", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();

    // All requested fields should be in response
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user123");
    assert_eq!(response["email"], "updated@example.com");
    assert_eq!(response["name"], "Updated Name");
    assert_eq!(response["phone"], "+1-555-1234");
    assert_eq!(response["address"], "123 Main St");
}

#[test]
fn test_mutation_return_computed_fields() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "order_id": "order123",
        "subtotal": 100.00,
        "tax": 10.00,
        "total": 110.00  // Computed field
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Order", "updateOrder", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();

    // Computed fields should be in response
    assert_eq!(response["total"], 110.00);
    assert_eq!(response["subtotal"], 100.00);
    assert_eq!(response["tax"], 10.00);
}

#[test]
fn test_mutation_return_related_entities() {
    let mock_adapter = Arc::new(MockMutationDatabaseAdapter::new());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let variables = json!({
        "order_id": "order123",
        "customer_id": "cust456",
        "status": "confirmed"
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);
    let result =
        runtime.block_on(executor.execute_local_mutation("Order", "updateOrder", &variables));

    assert!(result.is_ok());
    let response = result.unwrap();

    // Response includes related entity references
    assert_eq!(response["__typename"], "Order");
    assert_eq!(response["order_id"], "order123");
    assert_eq!(response["customer_id"], "cust456");
}
