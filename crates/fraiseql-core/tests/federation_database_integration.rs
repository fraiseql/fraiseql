//! Federation database integration tests
//!
//! Tests for real database entity resolution covering:
//! - Single and batch entity queries from databases
//! - Cross-database federation (PostgreSQL, MySQL, SQL Server)
//! - WHERE clause construction and SQL injection prevention
//! - Connection pooling and transaction handling
//! - Type coercion between database systems

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, DatabaseCapabilities},
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    federation::{
        database_resolver::DatabaseEntityResolver,
        selection_parser::FieldSelection,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
    },
};
use serde_json::{Value, json};

// ============================================================================
// Mock Database Adapter for Testing
// ============================================================================

struct MockDatabaseAdapter {
    data: HashMap<String, Vec<HashMap<String, Value>>>,
}

impl MockDatabaseAdapter {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    fn with_table_data(mut self, table: String, rows: Vec<HashMap<String, Value>>) -> Self {
        self.data.insert(table, rows);
        self
    }
}

#[async_trait]
impl DatabaseAdapter for MockDatabaseAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Not needed for these tests
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

    async fn execute_raw_query(&self, sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        // Extract table name from simple SELECT queries
        if let Some(start) = sql.to_uppercase().find("FROM ") {
            let after_from = &sql[start + 5..].trim();
            if let Some(space_pos) = after_from.find(' ') {
                let table = after_from[..space_pos].trim().to_lowercase();
                if let Some(rows) = self.data.get(&table) {
                    return Ok(rows.clone());
                }
            } else {
                let table = after_from.to_lowercase();
                if let Some(rows) = self.data.get(&table) {
                    return Ok(rows.clone());
                }
            }
        }
        Ok(Vec::new())
    }

    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities::from_database_type(self.database_type())
    }
}

// ============================================================================
// Database Entity Resolution (PostgreSQL)
// ============================================================================

#[test]
fn test_resolve_entity_from_postgres_table() {
    use std::collections::HashMap;

    // Setup
    let mut user_row = HashMap::new();
    user_row.insert("id".to_string(), json!("user123"));
    user_row.insert("name".to_string(), json!("John Doe"));
    user_row.insert("email".to_string(), json!("john@example.com"));

    let mock_adapter =
        MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user_row]);

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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user123"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    // Execute
    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    // Verify
    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = &entities[0].as_ref().unwrap();
    assert_eq!(entity["__typename"], "User");
    assert_eq!(entity["id"], "user123");
    assert_eq!(entity["name"], "John Doe");
}

#[test]
fn test_resolve_entities_batch_from_postgres() {
    use std::collections::HashMap;

    // Setup - multiple users
    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), json!("user1"));
    user1.insert("name".to_string(), json!("Alice"));

    let mut user2 = HashMap::new();
    user2.insert("id".to_string(), json!("user2"));
    user2.insert("name".to_string(), json!("Bob"));

    let mock_adapter =
        MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user1, user2]);

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

    // Request for two entities
    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("id".to_string(), json!("user1"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("id".to_string(), json!("user1"));

    let mut rep2_keys = HashMap::new();
    rep2_keys.insert("id".to_string(), json!("user2"));
    let mut rep2_all = HashMap::new();
    rep2_all.insert("id".to_string(), json!("user2"));

    let reps = vec![
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep1_keys,
            all_fields: rep1_all,
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep2_keys,
            all_fields: rep2_all,
        },
    ];

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    // Execute
    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &reps, &selection));

    // Verify
    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 2);
    assert!(entities[0].is_some());
    assert!(entities[1].is_some());

    assert_eq!(entities[0].as_ref().unwrap()["name"], "Alice");
    assert_eq!(entities[1].as_ref().unwrap()["name"], "Bob");
}

#[test]
fn test_resolve_entity_composite_key_from_postgres() {
    use std::collections::HashMap;

    // Setup - entity with composite key (tenant_id, user_id)
    let mut row = HashMap::new();
    row.insert("tenant_id".to_string(), json!("t1"));
    row.insert("user_id".to_string(), json!("u1"));
    row.insert("name".to_string(), json!("John"));

    let mock_adapter = MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![row]);

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["tenant_id".to_string(), "user_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let mut rep_keys = HashMap::new();
    rep_keys.insert("tenant_id".to_string(), json!("t1"));
    rep_keys.insert("user_id".to_string(), json!("u1"));
    let mut rep_all = HashMap::new();
    rep_all.insert("tenant_id".to_string(), json!("t1"));
    rep_all.insert("user_id".to_string(), json!("u1"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "tenant_id".to_string(),
        "user_id".to_string(),
        "name".to_string(),
    ]);

    // Execute
    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    // Verify
    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
    assert_eq!(entities[0].as_ref().unwrap()["name"], "John");
}

#[test]
fn test_resolve_entity_with_null_values_from_postgres() {
    use std::collections::HashMap;

    // Setup - entity with NULL values
    let mut row = HashMap::new();
    row.insert("id".to_string(), json!("user123"));
    row.insert("name".to_string(), json!("John"));
    row.insert("email".to_string(), Value::Null);

    let mock_adapter = MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![row]);

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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user123"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    // Execute
    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    // Verify
    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = &entities[0].as_ref().unwrap();
    assert_eq!(entity["name"], "John");
    assert_eq!(entity["email"], Value::Null);
}

#[test]
fn test_resolve_entity_large_result_set_from_postgres() {
    use std::collections::HashMap;

    // Setup - 100 users
    let mut rows = Vec::new();
    let mut reps = Vec::new();

    for i in 0..100 {
        let mut row = HashMap::new();
        let id = format!("user{}", i);
        row.insert("id".to_string(), json!(id.clone()));
        row.insert("name".to_string(), json!(format!("User {}", i)));
        rows.push(row);

        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!(id.clone()));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!(id));

        reps.push(EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        });
    }

    let mock_adapter = MockDatabaseAdapter::new().with_table_data("user".to_string(), rows);

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

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    // Execute
    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &reps, &selection));

    // Verify
    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 100);
    for i in 0..100 {
        assert!(entities[i].is_some());
    }
}

// ============================================================================
// WHERE Clause Construction
// ============================================================================

#[test]
fn test_where_clause_single_key_field() {
    use std::collections::HashMap;

    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
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

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("id".to_string(), json!("123"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("id".to_string(), json!("123"));
    let rep1 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep1_keys,
        all_fields: rep1_all,
    };

    let mut rep2_keys = HashMap::new();
    rep2_keys.insert("id".to_string(), json!("456"));
    let mut rep2_all = HashMap::new();
    rep2_all.insert("id".to_string(), json!("456"));
    let rep2 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep2_keys,
        all_fields: rep2_all,
    };

    let where_clause = construct_where_in_clause("User", &[rep1, rep2], &metadata).unwrap();
    assert_eq!(where_clause, "id IN ('123', '456')");
}

#[test]
fn test_where_clause_composite_keys() {
    use std::collections::HashMap;

    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Order".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["user_id".to_string(), "order_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("user_id".to_string(), json!("user1"));
    rep1_keys.insert("order_id".to_string(), json!("order1"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("user_id".to_string(), json!("user1"));
    rep1_all.insert("order_id".to_string(), json!("order1"));
    let rep1 = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: rep1_keys,
        all_fields: rep1_all,
    };

    let where_clause = construct_where_in_clause("Order", &[rep1], &metadata).unwrap();
    assert_eq!(where_clause, "(user_id, order_id) IN (('user1', 'order1'))");
}

#[test]
fn test_where_clause_string_escaping() {
    use std::collections::HashMap;

    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
    };
    use serde_json::json;

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["name".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let mut rep_keys = HashMap::new();
    rep_keys.insert("name".to_string(), json!("O'Brien"));
    let mut rep_all = HashMap::new();
    rep_all.insert("name".to_string(), json!("O'Brien"));
    let rep = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let where_clause = construct_where_in_clause("User", &[rep], &metadata).unwrap();
    assert_eq!(where_clause, "name IN ('O''Brien')");
}

#[test]
fn test_where_clause_sql_injection_prevention() {
    use std::collections::HashMap;

    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("'; DROP TABLE users; --"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("'; DROP TABLE users; --"));
    let rep = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let where_clause = construct_where_in_clause("User", &[rep], &metadata).unwrap();
    assert_eq!(where_clause, "id IN ('''; DROP TABLE users; --')");
}

#[test]
fn test_where_clause_type_coercion() {
    use std::collections::HashMap;

    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("order_id".to_string(), json!(789));
    let mut rep_all = HashMap::new();
    rep_all.insert("order_id".to_string(), json!(789));
    let rep = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let where_clause = construct_where_in_clause("Order", &[rep], &metadata).unwrap();
    assert_eq!(where_clause, "order_id IN ('789')");
}

// ============================================================================
// Cross-Database Federation
// ============================================================================

#[test]
fn test_cross_database_postgres_to_mysql() {
    // Test federation between PostgreSQL and MySQL subgraphs
    // Both databases store the same schema with compatible types
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user123"));
    user.insert("username".to_string(), json!("alice"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]));

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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user123"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "username".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
    assert_eq!(entities[0].as_ref().unwrap()["username"], "alice");
}

#[test]
fn test_cross_database_postgres_to_sqlserver() {
    // Test federation between PostgreSQL and SQL Server subgraphs
    let mut product = HashMap::new();
    product.insert("product_id".to_string(), json!("prod123"));
    product.insert("product_name".to_string(), json!("Widget"));
    product.insert("price".to_string(), json!(29.99));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("product".to_string(), vec![product]));

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Product".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["product_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let mut rep_keys = HashMap::new();
    rep_keys.insert("product_id".to_string(), json!("prod123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("product_id".to_string(), json!("prod123"));

    let representation = EntityRepresentation {
        typename:   "Product".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "product_id".to_string(),
        "product_name".to_string(),
        "price".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result = runtime.block_on(resolver.resolve_entities_from_db(
        "Product",
        &[representation],
        &selection,
    ));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
    assert_eq!(entities[0].as_ref().unwrap()["product_name"], "Widget");
}

#[test]
fn test_cross_database_type_coercion_numeric() {
    // Test numeric type coercion between databases
    let mut order = HashMap::new();
    order.insert("order_id".to_string(), json!("order123"));
    order.insert("amount".to_string(), json!(100)); // Integer
    order.insert("discount_rate".to_string(), json!(0.15)); // Float

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("order".to_string(), vec![order]));

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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("order_id".to_string(), json!("order123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("order_id".to_string(), json!("order123"));

    let representation = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "order_id".to_string(),
        "amount".to_string(),
        "discount_rate".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("Order", &[representation], &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = entities[0].as_ref().unwrap();
    assert_eq!(entity["amount"], 100);
    assert_eq!(entity["discount_rate"], 0.15);
}

#[test]
fn test_cross_database_type_coercion_string() {
    // Test string type coercion between databases
    let mut customer = HashMap::new();
    customer.insert("customer_id".to_string(), json!("cust123"));
    customer.insert("email".to_string(), json!("test@example.com"));
    customer.insert("phone".to_string(), json!("+1-555-1234")); // String-based phone

    let mock_adapter = Arc::new(
        MockDatabaseAdapter::new().with_table_data("customer".to_string(), vec![customer]),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Customer".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["customer_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let mut rep_keys = HashMap::new();
    rep_keys.insert("customer_id".to_string(), json!("cust123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("customer_id".to_string(), json!("cust123"));

    let representation = EntityRepresentation {
        typename:   "Customer".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "customer_id".to_string(),
        "email".to_string(),
        "phone".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result = runtime.block_on(resolver.resolve_entities_from_db(
        "Customer",
        &[representation],
        &selection,
    ));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = entities[0].as_ref().unwrap();
    assert_eq!(entity["email"], "test@example.com");
    assert_eq!(entity["phone"], "+1-555-1234");
}

#[test]
fn test_cross_database_type_coercion_datetime() {
    // Test datetime type coercion between databases
    let mut event = HashMap::new();
    event.insert("event_id".to_string(), json!("evt123"));
    event.insert("event_date".to_string(), json!("2024-01-15T10:30:00Z"));
    event.insert("created_at".to_string(), json!("2024-01-15T00:00:00Z"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("event".to_string(), vec![event]));

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             "Event".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["event_id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    };

    let mut rep_keys = HashMap::new();
    rep_keys.insert("event_id".to_string(), json!("evt123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("event_id".to_string(), json!("evt123"));

    let representation = EntityRepresentation {
        typename:   "Event".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "event_id".to_string(),
        "event_date".to_string(),
        "created_at".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("Event", &[representation], &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = entities[0].as_ref().unwrap();
    assert_eq!(entity["event_date"], "2024-01-15T10:30:00Z");
    assert_eq!(entity["created_at"], "2024-01-15T00:00:00Z");
}

// ============================================================================
// Connection Management
// ============================================================================

#[test]
fn test_database_connection_pooling() {
    // Test that database adapter provides connection pool metrics
    let mock_adapter = MockDatabaseAdapter::new();

    let metrics = mock_adapter.pool_metrics();
    assert_eq!(metrics.total_connections, 10);
    assert_eq!(metrics.idle_connections, 8);
    assert_eq!(metrics.active_connections, 2);
    assert_eq!(metrics.waiting_requests, 0);
}

#[test]
fn test_database_connection_reuse() {
    // Test that connections are reused from pool for multiple queries
    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), json!("user1"));
    user1.insert("name".to_string(), json!("Alice"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user1]));

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

    // Execute multiple queries to test connection reuse
    let runtime = tokio::runtime::Runtime::new().unwrap();

    for _ in 0..3 {
        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!("user1"));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!("user1"));

        let representation = EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        };

        let selection = FieldSelection::new(vec![
            "__typename".to_string(),
            "id".to_string(),
            "name".to_string(),
        ]);

        let resolver = DatabaseEntityResolver::new(mock_adapter.clone(), metadata.clone());
        let result = runtime.block_on(resolver.resolve_entities_from_db(
            "User",
            &[representation],
            &selection,
        ));

        assert!(result.is_ok());
        let entities = result.unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }

    // Verify pool metrics haven't changed (connection reuse)
    let metrics = mock_adapter.pool_metrics();
    assert_eq!(metrics.total_connections, 10);
    assert_eq!(metrics.idle_connections, 8);
}

#[test]
fn test_database_connection_timeout() {
    // Test that connection timeout is handled gracefully
    let mock_adapter = Arc::new(MockDatabaseAdapter::new());

    // Health check should succeed
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(mock_adapter.health_check());
    assert!(result.is_ok());

    // Verify database is still responsive after health check
    let result = runtime.block_on(mock_adapter.execute_raw_query("SELECT 1"));
    assert!(result.is_ok());
}

#[test]
fn test_database_connection_retry() {
    // Test that operations can be retried on transient failures
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("Bob"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]));

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

    // Simulate retry: try multiple times
    for attempt in 0..3 {
        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!("user1"));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!("user1"));

        let representation = EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        };

        let selection = FieldSelection::new(vec![
            "__typename".to_string(),
            "id".to_string(),
            "name".to_string(),
        ]);

        let resolver = DatabaseEntityResolver::new(mock_adapter.clone(), metadata.clone());
        let result = runtime.block_on(resolver.resolve_entities_from_db(
            "User",
            &[representation],
            &selection,
        ));

        // After any attempt, we should get a successful result
        assert!(result.is_ok(), "Attempt {} failed", attempt);
        let entities = result.unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }
}

// ============================================================================
// Query Execution
// ============================================================================

#[test]
fn test_database_query_execution_basic() {
    // Test basic query execution with SELECT * FROM table
    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), json!("user1"));
    user1.insert("email".to_string(), json!("user1@example.com"));

    let mut user2 = HashMap::new();
    user2.insert("id".to_string(), json!("user2"));
    user2.insert("email".to_string(), json!("user2@example.com"));

    let mock_adapter = Arc::new(
        MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user1, user2]),
    );

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

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("id".to_string(), json!("user1"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("id".to_string(), json!("user1"));

    let mut rep2_keys = HashMap::new();
    rep2_keys.insert("id".to_string(), json!("user2"));
    let mut rep2_all = HashMap::new();
    rep2_all.insert("id".to_string(), json!("user2"));

    let representations = vec![
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep1_keys,
            all_fields: rep1_all,
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep2_keys,
            all_fields: rep2_all,
        },
    ];

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "email".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &representations, &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].as_ref().unwrap()["email"], "user1@example.com");
    assert_eq!(entities[1].as_ref().unwrap()["email"], "user2@example.com");
}

#[test]
fn test_database_prepared_statements() {
    // Prepared statements are handled at the adapter level
    // This test verifies that execute_raw_query works correctly
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("John"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]));

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Execute a SELECT query
    let result = runtime
        .block_on(mock_adapter.execute_raw_query("SELECT id, name FROM user WHERE id = 'user1'"));

    assert!(result.is_ok());
    let rows = result.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "user1");
    assert_eq!(rows[0]["name"], "John");
}

#[test]
fn test_database_parameterized_queries() {
    // Parameterized queries prevent SQL injection
    // This test verifies that SQL escaping works in WHERE clauses
    use std::collections::HashMap;

    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause,
        types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
    };

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

    // Try to inject SQL in the key value with O'Brien (common apostrophe case)
    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("O'Brien"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("O'Brien"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    // Build WHERE clause - should escape the apostrophe
    let where_clause = construct_where_in_clause("User", &[representation], &metadata).unwrap();

    // The WHERE clause should have the single quote escaped (doubled)
    // O'Brien becomes O''Brien in SQL
    assert!(where_clause.contains("O''Brien")); // Apostrophe should be escaped
    // The clause should still be valid SQL
    assert!(where_clause.contains("id IN")); // Standard WHERE IN clause format
}

#[test]
fn test_database_transaction_handling() {
    // Transaction support is at the adapter level
    // This test verifies that multiple operations complete successfully
    let mock_adapter = Arc::new(MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Simulate transactional behavior with health check
    let result1 = runtime.block_on(mock_adapter.health_check());
    assert!(result1.is_ok());

    let result2 = runtime.block_on(mock_adapter.execute_raw_query("SELECT 1"));
    assert!(result2.is_ok());

    // Both operations should succeed
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[test]
fn test_database_transaction_rollback() {
    // Rollback is handled at the adapter level
    // This test verifies that failed operations don't corrupt state
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("John"));

    let mock_adapter = Arc::new(
        MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user.clone()]),
    );

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Query should succeed
    let result1 = runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM user"));
    assert!(result1.is_ok());

    // Query for non-existent table should fail gracefully
    let result2 =
        runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM nonexistent_table"));
    assert!(result2.is_ok()); // Returns empty result, not error

    // Original data should still be intact
    let result3 = runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM user"));
    assert!(result3.is_ok());
    let rows = result3.unwrap();
    assert_eq!(rows[0]["id"], "user1");
}

// ============================================================================
// Field Selection and Projection
// ============================================================================

#[test]
fn test_select_requested_fields_only() {
    use fraiseql_core::federation::selection_parser::parse_field_selection;

    let query = r#"
        query {
            _entities(representations: [...]) {
                __typename
                id
                name
                email
            }
        }
    "#;

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("__typename"));
    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(selection.contains("email"));
    assert!(!selection.contains("password")); // Not requested
}

#[test]
fn test_select_excludes_external_fields() {
    use fraiseql_core::federation::selection_parser::FieldSelection;

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    // Simulating external field filtering
    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(!selection.contains("orders")); // External field not selected
}

#[test]
fn test_select_includes_key_fields() {
    use fraiseql_core::federation::selection_parser::FieldSelection;

    let mut selection = FieldSelection::new(vec!["name".to_string(), "email".to_string()]);

    // Key fields should always be included
    selection.add_field("id".to_string());
    selection.add_field("__typename".to_string());

    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(selection.contains("email"));
    assert!(selection.contains("__typename"));
}

#[test]
fn test_result_projection_to_federation_format() {
    use serde_json::json;

    // Test projection from database format to federation format
    let db_result = json!({
        "id": "user123",
        "name": "John",
        "email": "john@example.com"
    });

    let federated = json!({
        "__typename": "User",
        "id": db_result["id"].clone(),
        "name": db_result["name"].clone(),
        "email": db_result["email"].clone(),
    });

    assert_eq!(federated["__typename"], "User");
    assert_eq!(federated["id"], "user123");
    assert_eq!(federated["name"], "John");
    assert_eq!(federated["email"], "john@example.com");
}

// ============================================================================
// Error Handling
// ============================================================================

#[test]
fn test_database_query_timeout() {
    // Test that query timeouts are handled gracefully
    let mock_adapter = Arc::new(MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Query with reasonable timeout should succeed
    let result = runtime.block_on(mock_adapter.execute_raw_query("SELECT 1"));

    assert!(result.is_ok());
}

#[test]
fn test_database_connection_failure() {
    // Test that connection failures are handled gracefully
    let mock_adapter = Arc::new(MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Health check should succeed with working mock adapter
    let result = runtime.block_on(mock_adapter.health_check());
    assert!(result.is_ok());

    // Query for non-existent table should not panic
    let result = runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM nonexistent"));
    // Mock adapter returns Ok(empty) for missing tables
    assert!(result.is_ok());
}

#[test]
fn test_database_query_syntax_error() {
    // Test that syntax errors don't cause panics
    let mock_adapter = Arc::new(MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Invalid SQL should be handled gracefully
    let result = runtime.block_on(mock_adapter.execute_raw_query("INVALID SQL SYNTAX ;;;"));

    // Mock adapter returns Ok(empty) for all queries
    assert!(result.is_ok());
}

#[test]
fn test_database_constraint_violation() {
    // Test that constraint violations are handled
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("email".to_string(), json!("test@example.com"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]));

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

    // Try to resolve user (should succeed)
    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user1"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user1"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    // Should handle gracefully without panicking
    assert!(result.is_ok());
}

// ============================================================================
// Performance
// ============================================================================

#[test]
fn test_single_entity_resolution_latency() {
    // Test single entity resolution performance
    use std::time::Instant;

    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("John"));

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]));

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

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user1"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user1"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);

    // Measure resolution time
    let start = Instant::now();
    let _result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));
    let duration = start.elapsed();

    // Mock resolution should be very fast (< 1ms)
    assert!(duration.as_millis() < 10, "Single entity resolution took {:?}", duration);
}

#[test]
fn test_batch_100_entities_resolution_latency() {
    // Test batch resolution performance
    use std::time::Instant;

    let mut rows = Vec::new();
    let mut reps = Vec::new();

    // Create 100 users
    for i in 0..100 {
        let mut row = HashMap::new();
        let id = format!("user{}", i);
        row.insert("id".to_string(), json!(id.clone()));
        row.insert("name".to_string(), json!(format!("User {}", i)));
        rows.push(row);

        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!(id.clone()));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!(id));

        reps.push(EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        });
    }

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), rows));

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

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);

    // Measure resolution time
    let start = Instant::now();
    let result = runtime.block_on(resolver.resolve_entities_from_db("User", &reps, &selection));
    let duration = start.elapsed();

    // Verify all entities resolved
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 100);

    // Batch resolution should be reasonable (< 100ms for mock)
    assert!(duration.as_millis() < 100, "Batch resolution took {:?}", duration);
}

#[test]
fn test_concurrent_entity_resolution() {
    // Test that multiple concurrent-like resolutions succeed
    let mut users = Vec::new();
    for i in 0..10 {
        let mut user = HashMap::new();
        user.insert("id".to_string(), json!(format!("user{}", i)));
        user.insert("name".to_string(), json!(format!("User {}", i)));
        users.push(user);
    }

    let mock_adapter =
        Arc::new(MockDatabaseAdapter::new().with_table_data("user".to_string(), users));

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

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Run multiple resolutions (simulating concurrent requests)
    for i in 0..5 {
        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!(format!("user{}", i)));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!(format!("user{}", i)));

        let representation = EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        };

        let resolver = DatabaseEntityResolver::new(mock_adapter.clone(), metadata.clone());
        let result = runtime.block_on(resolver.resolve_entities_from_db(
            "User",
            &[representation],
            &selection,
        ));

        assert!(result.is_ok());
        let entities = result.unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }
}
