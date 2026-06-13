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

    // Update order (subgraph 1, local). Seed the row first so the read-back
    // UPDATE matches an existing entity (#430).
    let order_vars = json!({"order_id": "order123", "status": "confirmed"});
    executor
        .execute_local_mutation("Order", "createOrder", &order_vars)
        .await
        .unwrap_or_else(|e| panic!("seed createOrder failed: {e}"));
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

    // Update user in subgraph 1 (local). Seed the row, then update it and verify
    // the response is read back from the database, not echoed from input (#430).
    executor
        .execute_local_mutation("User", "createUser", &json!({"id": "user123", "status": "new"}))
        .await
        .unwrap_or_else(|e| panic!("seed createUser failed: {e}"));
    let user_vars = json!({"id": "user123", "status": "verified"});
    let updated = executor
        .execute_local_mutation("User", "updateUser", &user_vars)
        .await
        .unwrap_or_else(|e| panic!("execute_local_mutation(User/updateUser) failed: {e}"));
    assert_eq!(updated["__typename"], "User");
    assert_eq!(updated["status"], "verified", "read-back must reflect the updated DB row");

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

    // Update author (local). Seed first so the read-back UPDATE matches (#430).
    let author_vars = json!({"author_id": "author1", "name": "Updated Author"});
    executor
        .execute_local_mutation("Author", "createAuthor", &author_vars)
        .await
        .unwrap_or_else(|e| panic!("seed createAuthor failed: {e}"));
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

    // Seed first so the read-back UPDATE matches an existing account (#430).
    executor
        .execute_local_mutation("Account", "createAccount", &variables)
        .await
        .unwrap_or_else(|e| panic!("seed createAccount failed: {e}"));
    executor
        .execute_local_mutation("Account", "updateAccount", &variables)
        .await
        .unwrap_or_else(|e| panic!("execute_local_mutation(Account/updateAccount) failed: {e}"));
}

#[tokio::test]
async fn test_mutation_absent_local_entity_fails_loud() {
    // A local UPDATE against a row that does not exist must fail loud with a
    // not-found error (0-row read-back), not silently "succeed" by echoing the
    // input — the failure semantics a cross-subgraph coordinator relies on to
    // roll back (#430).
    let metadata = common::metadata_single_key("Transaction", "txn_id");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("transaction", &["txn_id text", "amount numeric"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_absent_local_entity_fails_loud: no postgres");
        return;
    };

    // The `transaction` table is empty — this UPDATE matches no row.
    let variables = json!({
        "txn_id": "txn123",
        "amount": 100.00
    });

    let err = executor
        .execute_local_mutation("Transaction", "updateTransaction", &variables)
        .await
        .expect_err("an UPDATE matching no row must error, not fabricate success");
    assert!(
        err.to_string().contains("not found"),
        "0-row UPDATE must be a not-found error, got: {err}"
    );
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
                // Seed each row so the read-back UPDATE matches (#430).
                exec.execute_local_mutation("User", "createUser", &variables).await?;
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

// ── #430 read-back acceptance ─────────────────────────────────────────────────

#[tokio::test]
async fn local_update_reads_back_db_row_not_input_echo() {
    // The `version` column has a DB default and is never present in the input;
    // its appearance in the response proves the row is read back from the
    // database (RETURNING *), not echoed from the input variables.
    let metadata = common::metadata_single_key("Widget", "id");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("widget", &["id text", "status text", "version int not null default 7"])],
    )
    .await
    else {
        eprintln!("SKIP local_update_reads_back_db_row_not_input_echo: no postgres");
        return;
    };

    executor
        .execute_local_mutation("Widget", "createWidget", &json!({"id": "w1", "status": "new"}))
        .await
        .unwrap_or_else(|e| panic!("seed createWidget failed: {e}"));
    let resp = executor
        .execute_local_mutation("Widget", "updateWidget", &json!({"id": "w1", "status": "active"}))
        .await
        .unwrap_or_else(|e| panic!("updateWidget failed: {e}"));

    assert_eq!(resp["__typename"], "Widget");
    assert_eq!(resp["status"], "active", "read-back reflects the updated value");
    assert_eq!(
        resp["version"], 7,
        "the DB-default column must come from RETURNING *, not the input echo"
    );
}

#[tokio::test]
async fn local_delete_absent_row_is_not_found() {
    let metadata = common::metadata_single_key("Widget", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("widget", &["id text"])]).await
    else {
        eprintln!("SKIP local_delete_absent_row_is_not_found: no postgres");
        return;
    };

    let err = executor
        .execute_local_mutation("Widget", "deleteWidget", &json!({"id": "ghost"}))
        .await
        .expect_err("a DELETE matching no row must error");
    assert!(
        err.to_string().contains("not found"),
        "0-row DELETE must be a not-found error, got: {err}"
    );
}

#[tokio::test]
async fn local_delete_returns_the_deleted_row() {
    let metadata = common::metadata_single_key("Widget", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("widget", &["id text", "status text"])]).await
    else {
        eprintln!("SKIP local_delete_returns_the_deleted_row: no postgres");
        return;
    };

    executor
        .execute_local_mutation("Widget", "createWidget", &json!({"id": "w9", "status": "live"}))
        .await
        .unwrap_or_else(|e| panic!("seed createWidget failed: {e}"));
    let resp = executor
        .execute_local_mutation("Widget", "deleteWidget", &json!({"id": "w9"}))
        .await
        .unwrap_or_else(|e| panic!("deleteWidget failed: {e}"));

    assert_eq!(resp["__typename"], "Widget");
    assert_eq!(resp["id"], "w9", "DELETE ... RETURNING * returns the removed row");
    assert_eq!(resp["status"], "live");
}
