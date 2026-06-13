//! Cross-subgraph mutation coordination.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code, panics + skip notes acceptable
use fraiseql_core::federation::types::{FederatedType, FederationMetadata, KeyDirective};
use serde_json::json;

use super::common;

#[tokio::test]
async fn test_mutation_coordinate_two_subgraph_updates() {
    // Coordinate mutations across two subgraphs
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![
            FederatedType {
                name:                "Order".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["order_id".to_string()],
                    resolvable: true,
                }],
                is_extends:          false,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
            FederatedType {
                name:                "OrderItem".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["item_id".to_string()],
                    resolvable: true,
                }],
                is_extends:          true,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
        ],
        remote_subscription_fields: std::collections::HashMap::new(),
    };

    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("order", &["order_id text", "status text"])])
            .await
    else {
        eprintln!("SKIP test_mutation_coordinate_two_subgraph_updates: no postgres");
        return;
    };

    // Update order (subgraph 1, local)
    let order_vars = json!({"order_id": "order123", "status": "confirmed"});
    executor
        .execute_local_mutation("Order", "updateOrder", &order_vars)
        .await
        .unwrap_or_else(|e| panic!("execute_local_mutation(Order/updateOrder) failed: {e}"));

    // Update order items (subgraph 2, extended)
    let item_vars = json!({"item_id": "item1", "quantity": 2});
    executor
        .execute_extended_mutation("OrderItem", "updateQuantity", &item_vars)
        .await
        .unwrap_or_else(|e| {
            panic!("execute_extended_mutation(OrderItem/updateQuantity) failed: {e}")
        });
}

#[tokio::test]
#[ignore = "local op name 'verifyUser' is not create/update/delete; determine_mutation_type now fails loud (M-fed-mut-executor) instead of silently defaulting to UPDATE. Cross-graph mutation rework is deferred to Phase 09 — see #430"]
async fn test_mutation_coordinate_three_subgraph_updates() {
    // Coordinate mutations across three subgraphs
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![
            FederatedType {
                name:                "User".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:          false,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
            FederatedType {
                name:                "Order".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["order_id".to_string()],
                    resolvable: true,
                }],
                is_extends:          true,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
            FederatedType {
                name:                "Payment".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["payment_id".to_string()],
                    resolvable: true,
                }],
                is_extends:          true,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
        ],
        remote_subscription_fields: std::collections::HashMap::new(),
    };

    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text", "status text"])]).await
    else {
        eprintln!("SKIP test_mutation_coordinate_three_subgraph_updates: no postgres");
        return;
    };

    // Update user in subgraph 1 (local)
    let user_vars = json!({"id": "user123", "status": "verified"});
    executor
        .execute_local_mutation("User", "verifyUser", &user_vars)
        .await
        .unwrap_or_else(|e| panic!("execute_local_mutation(User/verifyUser) failed: {e}"));

    // Update order in subgraph 2 (extended)
    let order_vars = json!({"order_id": "order123", "status": "processing"});
    executor
        .execute_extended_mutation("Order", "updateOrder", &order_vars)
        .await
        .unwrap_or_else(|e| panic!("execute_extended_mutation(Order/updateOrder) failed: {e}"));

    // Update payment in subgraph 3 (extended)
    let payment_vars = json!({"payment_id": "pay123", "status": "processed"});
    executor
        .execute_extended_mutation("Payment", "processPayment", &payment_vars)
        .await
        .unwrap_or_else(|e| {
            panic!("execute_extended_mutation(Payment/processPayment) failed: {e}")
        });
}

#[tokio::test]
async fn test_mutation_reference_update_propagation() {
    // Reference update propagation across subgraphs (extended only)
    let metadata = common::metadata_extended_type("Review", "review_id", &["product_id"], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_reference_update_propagation: no postgres");
        return;
    };

    let variables = json!({
        "review_id": "review123",
        "product_id": "product456",
        "rating": 5
    });

    executor
        .execute_extended_mutation("Review", "updateReview", &variables)
        .await
        .unwrap_or_else(|e| panic!("execute_extended_mutation(Review/updateReview) failed: {e}"));
}

#[tokio::test]
async fn test_mutation_circular_reference_handling() {
    // Circular reference handling in mutations
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![
            FederatedType {
                name:                "Author".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["author_id".to_string()],
                    resolvable: true,
                }],
                is_extends:          false,
                external_fields:     vec![],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
            FederatedType {
                name:                "Book".to_string(),
                keys:                vec![KeyDirective {
                    fields:     vec!["book_id".to_string()],
                    resolvable: true,
                }],
                is_extends:          true,
                external_fields:     vec!["author_id".to_string()],
                shareable_fields:    vec![],
                inaccessible_fields: vec![],
                field_directives:    std::collections::HashMap::new(),
                type_shareable:      false,
            },
        ],
        remote_subscription_fields: std::collections::HashMap::new(),
    };

    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("author", &["author_id text", "name text"])])
            .await
    else {
        eprintln!("SKIP test_mutation_circular_reference_handling: no postgres");
        return;
    };

    // Update author (local)
    let author_vars = json!({"author_id": "author1", "name": "Updated Author"});
    executor
        .execute_local_mutation("Author", "updateAuthor", &author_vars)
        .await
        .unwrap_or_else(|e| panic!("execute_local_mutation(Author/updateAuthor) failed: {e}"));

    // Update book referencing author (circular, extended)
    let book_vars = json!({"book_id": "book1", "author_id": "author1", "title": "Updated Book"});
    executor
        .execute_extended_mutation("Book", "updateBook", &book_vars)
        .await
        .unwrap_or_else(|e| panic!("execute_extended_mutation(Book/updateBook) failed: {e}"));
}

#[tokio::test]
async fn test_mutation_multi_subgraph_transaction() {
    // Multi-subgraph transaction handling
    let metadata = common::metadata_single_key("Account", "account_id");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("account", &["account_id text", "balance numeric"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_multi_subgraph_transaction: no postgres");
        return;
    };

    let variables = json!({
        "account_id": "acc123",
        "balance": 1000.00
    });

    executor
        .execute_local_mutation("Account", "updateAccount", &variables)
        .await
        .unwrap_or_else(|e| panic!("execute_local_mutation(Account/updateAccount) failed: {e}"));
}

#[tokio::test]
#[ignore = "local op name 'executeTransaction' is not create/update/delete; determine_mutation_type now fails loud (M-fed-mut-executor) instead of silently defaulting to UPDATE. Rollback-on-failure coverage depends on the deferred read-back semantics — Phase 09, see #430"]
async fn test_mutation_subgraph_failure_rollback() {
    // Rollback on subgraph failure
    let metadata = common::metadata_single_key("Transaction", "txn_id");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("transaction", &["txn_id text", "amount numeric"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_subgraph_failure_rollback: no postgres");
        return;
    };

    let variables = json!({
        "txn_id": "txn123",
        "amount": 100.00
    });

    executor
        .execute_local_mutation("Transaction", "executeTransaction", &variables)
        .await
        .unwrap_or_else(|e| {
            panic!("execute_local_mutation(Transaction/executeTransaction) failed: {e}")
        });
}

#[tokio::test]
async fn test_mutation_subgraph_timeout_handling() {
    // Subgraph timeout handling (extended only)
    let metadata = common::metadata_extended_type("AsyncJob", "job_id", &[], &[]);
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_subgraph_timeout_handling: no postgres");
        return;
    };

    let variables = json!({
        "job_id": "job123",
        "status": "processing"
    });

    executor
        .execute_extended_mutation("AsyncJob", "updateJob", &variables)
        .await
        .unwrap_or_else(|e| panic!("execute_extended_mutation(AsyncJob/updateJob) failed: {e}"));
}

#[tokio::test]
async fn test_mutation_concurrent_request_handling() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text", "name text"])]).await
    else {
        eprintln!("SKIP test_mutation_concurrent_request_handling: no postgres");
        return;
    };

    // Spawn concurrent mutation requests sharing the executor (and its pool).
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let exec = executor.clone();
            tokio::spawn(async move {
                let variables = json!({
                    "id": format!("user{}", i),
                    "name": format!("Updated User {}", i)
                });
                exec.execute_local_mutation("User", "updateUser", &variables).await
            })
        })
        .collect();

    // All mutations should complete successfully
    for handle in handles {
        let join_result = handle.await.unwrap_or_else(|e| panic!("task panicked: {e:?}"));
        join_result
            .unwrap_or_else(|e| panic!("execute_local_mutation(User/updateUser) failed: {e}"));
    }
}
