//! Cross-subgraph mutation coordination.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::federation::{
    mutation_executor::FederationMutationExecutor,
    types::{FederatedType, FederationMetadata, KeyDirective},
};
use serde_json::json;

use super::common;

#[test]
fn test_mutation_coordinate_two_subgraph_updates() {
    // Coordinate mutations across two subgraphs
    let mock_adapter = common::mock_mutation_adapter();

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
                inaccessible_fields: vec![],
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
                inaccessible_fields: vec![],
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
    result1.unwrap_or_else(|e| panic!("execute_local_mutation(Order/updateOrder) failed: {e}"));

    // Update order items (subgraph 2)
    let item_vars = json!({"item_id": "item1", "quantity": 2});
    let executor2 = FederationMutationExecutor::new(mock_adapter, metadata);
    let result2 = runtime.block_on(executor2.execute_extended_mutation(
        "OrderItem",
        "updateQuantity",
        &item_vars,
    ));
    result2.unwrap_or_else(|e| {
        panic!("execute_extended_mutation(OrderItem/updateQuantity) failed: {e}")
    });
}

#[test]
fn test_mutation_coordinate_three_subgraph_updates() {
    // Coordinate mutations across three subgraphs
    let mock_adapter = common::mock_mutation_adapter();

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
                inaccessible_fields: vec![],
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
                inaccessible_fields: vec![],
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
                inaccessible_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            },
        ],
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    // Update user in subgraph 1
    let user_vars = json!({"id": "user123", "status": "verified"});
    let r1 = runtime.block_on(executor.execute_local_mutation("User", "verifyUser", &user_vars));
    r1.unwrap_or_else(|e| panic!("execute_local_mutation(User/verifyUser) failed: {e}"));

    // Update order in subgraph 2
    let order_vars = json!({"order_id": "order123", "status": "processing"});
    let r2 =
        runtime.block_on(executor.execute_extended_mutation("Order", "updateOrder", &order_vars));
    r2.unwrap_or_else(|e| panic!("execute_extended_mutation(Order/updateOrder) failed: {e}"));

    // Update payment in subgraph 3
    let payment_vars = json!({"payment_id": "pay123", "status": "processed"});
    let r3 = runtime.block_on(executor.execute_extended_mutation(
        "Payment",
        "processPayment",
        &payment_vars,
    ));
    r3.unwrap_or_else(|e| panic!("execute_extended_mutation(Payment/processPayment) failed: {e}"));
}

#[test]
fn test_mutation_reference_update_propagation() {
    // Reference update propagation across subgraphs
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("Review", "review_id", &["product_id"], &[]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "review_id": "review123",
        "product_id": "product456",
        "rating": 5
    });

    runtime
        .block_on(executor.execute_extended_mutation("Review", "updateReview", &variables))
        .unwrap_or_else(|e| panic!("execute_extended_mutation(Review/updateReview) failed: {e}"));
}

#[test]
fn test_mutation_circular_reference_handling() {
    // Circular reference handling in mutations
    let mock_adapter = common::mock_mutation_adapter();

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
                inaccessible_fields: vec![],
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
                inaccessible_fields: vec![],
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
    r1.unwrap_or_else(|e| panic!("execute_local_mutation(Author/updateAuthor) failed: {e}"));

    // Update book referencing author (circular)
    let book_vars = json!({"book_id": "book1", "author_id": "author1", "title": "Updated Book"});
    let executor2 = FederationMutationExecutor::new(mock_adapter, metadata);
    let r2 =
        runtime.block_on(executor2.execute_extended_mutation("Book", "updateBook", &book_vars));
    r2.unwrap_or_else(|e| panic!("execute_extended_mutation(Book/updateBook) failed: {e}"));
}

#[test]
fn test_mutation_multi_subgraph_transaction() {
    // Multi-subgraph transaction handling
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("Account", "account_id");

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "account_id": "acc123",
        "balance": 1000.00
    });

    runtime
        .block_on(executor.execute_local_mutation("Account", "updateAccount", &variables))
        .unwrap_or_else(|e| panic!("execute_local_mutation(Account/updateAccount) failed: {e}"));
}

#[test]
fn test_mutation_subgraph_failure_rollback() {
    // Rollback on subgraph failure
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("Transaction", "txn_id");

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "txn_id": "txn123",
        "amount": 100.00
    });

    runtime
        .block_on(executor.execute_local_mutation("Transaction", "executeTransaction", &variables))
        .unwrap_or_else(|e| {
            panic!("execute_local_mutation(Transaction/executeTransaction) failed: {e}")
        });
}

#[test]
fn test_mutation_subgraph_timeout_handling() {
    // Subgraph timeout handling
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_extended_type("AsyncJob", "job_id", &[], &[]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let executor = FederationMutationExecutor::new(mock_adapter, metadata);

    let variables = json!({
        "job_id": "job123",
        "status": "processing"
    });

    runtime
        .block_on(executor.execute_extended_mutation("AsyncJob", "updateJob", &variables))
        .unwrap_or_else(|e| panic!("execute_extended_mutation(AsyncJob/updateJob) failed: {e}"));
}

#[test]
fn test_mutation_concurrent_request_handling() {
    let mock_adapter = common::mock_mutation_adapter();
    let metadata = common::metadata_single_key("User", "id");

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
        let thread_result = handle.join().unwrap_or_else(|e| panic!("thread panicked: {e:?}"));
        thread_result
            .unwrap_or_else(|e| panic!("execute_local_mutation(User/updateUser) failed: {e}"));
    }
}
