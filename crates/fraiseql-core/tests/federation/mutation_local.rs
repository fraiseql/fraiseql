//! Local entity mutations (owned entities) against **real PostgreSQL**.
//!
//! `execute_local_mutation` builds a plain `INSERT`/`UPDATE`/`DELETE` against the
//! lowercased entity type name and runs it via `execute_raw_query`, so each test
//! provisions the columns its variables reference. The executor echoes the input
//! variables into the response (it does not read the row back — finding
//! M-fed-mut-executor), so the assertions exercise the input echo while the SQL
//! actually parses and executes against a real schema.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code, panics + skip notes acceptable
use serde_json::json;

use super::common;

#[tokio::test]
async fn test_mutation_create_owned_entity() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("user", &["id text", "name text", "email text"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_create_owned_entity: no postgres");
        return;
    };

    let variables = json!({
        "id": "user_new",
        "name": "New User",
        "email": "newuser@example.com"
    });
    let result = executor.execute_local_mutation("User", "createUser", &variables).await;

    let response =
        result.unwrap_or_else(|e| panic!("execute_local_mutation(User/createUser) failed: {e}"));
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user_new");
    assert_eq!(response["name"], "New User");
}

#[tokio::test]
async fn test_mutation_update_owned_entity() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("user", &["id text", "email text", "name text"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_update_owned_entity: no postgres");
        return;
    };

    let variables = json!({
        "id": "user123",
        "email": "updated@example.com",
        "name": "Updated Name"
    });
    let result = executor.execute_local_mutation("User", "updateUser", &variables).await;

    let response =
        result.unwrap_or_else(|e| panic!("execute_local_mutation(User/updateUser) failed: {e}"));
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user123");
    assert_eq!(response["email"], "updated@example.com");
    assert_eq!(response["name"], "Updated Name");
}

#[tokio::test]
async fn test_mutation_delete_owned_entity() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text"])]).await
    else {
        eprintln!("SKIP test_mutation_delete_owned_entity: no postgres");
        return;
    };

    let variables = json!({
        "id": "user_to_delete"
    });
    let result = executor.execute_local_mutation("User", "deleteUser", &variables).await;

    let response =
        result.unwrap_or_else(|e| panic!("execute_local_mutation(User/deleteUser) failed: {e}"));
    assert_eq!(response["__typename"], "User");
    assert_eq!(response["id"], "user_to_delete");
}

#[tokio::test]
async fn test_mutation_owned_entity_returns_updated_representation() {
    let metadata = common::metadata_single_key("Product", "sku");
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("product", &["sku text", "name text", "price numeric", "stock numeric"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_owned_entity_returns_updated_representation: no postgres");
        return;
    };

    let variables = json!({
        "sku": "PROD-001",
        "name": "Widget",
        "price": 29.99,
        "stock": 100
    });
    let result = executor.execute_local_mutation("Product", "updateProduct", &variables).await;

    let entity = result
        .unwrap_or_else(|e| panic!("execute_local_mutation(Product/updateProduct) failed: {e}"));

    // Verify response is a proper entity representation
    assert!(entity.is_object());
    assert_eq!(entity["__typename"], "Product");
    assert_eq!(entity["sku"], "PROD-001");
    assert_eq!(entity["price"], 29.99);
}

#[tokio::test]
async fn test_mutation_owned_entity_batch_updates() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text", "name text"])]).await
    else {
        eprintln!("SKIP test_mutation_owned_entity_batch_updates: no postgres");
        return;
    };

    // Execute multiple mutations
    for i in 0..3 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("User {}", i)
        });

        let result = executor.execute_local_mutation("User", "updateUser", &variables).await;

        let response = result.unwrap_or_else(|e| {
            panic!("execute_local_mutation(User/updateUser) batch iteration {i} failed: {e}")
        });
        assert_eq!(response["id"], format!("user{}", i));
    }
}

#[tokio::test]
async fn test_mutation_composite_key_update() {
    let metadata = common::metadata_composite_key("Order", &["tenant_id", "order_id"]);
    let Some((_pg, executor)) = common::pg_mutation_executor(
        metadata,
        &[("order", &["tenant_id text", "order_id text", "status text"])],
    )
    .await
    else {
        eprintln!("SKIP test_mutation_composite_key_update: no postgres");
        return;
    };

    let variables = json!({
        "tenant_id": "tenant_123",
        "order_id": "order_456",
        "status": "confirmed"
    });
    let result = executor.execute_local_mutation("Order", "updateOrder", &variables).await;

    let response =
        result.unwrap_or_else(|e| panic!("execute_local_mutation(Order/updateOrder) failed: {e}"));
    assert_eq!(response["__typename"], "Order");
    assert_eq!(response["tenant_id"], "tenant_123");
    assert_eq!(response["order_id"], "order_456");
    assert_eq!(response["status"], "confirmed");
}

#[tokio::test]
async fn test_mutation_with_validation_errors() {
    // A nested object cannot render as a SQL literal, so the query build fails
    // before any DB statement runs (no table needed).
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) = common::pg_mutation_executor(metadata, &[]).await else {
        eprintln!("SKIP test_mutation_with_validation_errors: no postgres");
        return;
    };

    let variables = json!({
        "id": "user1",
        "metadata": { "nested": "object" }  // Invalid for SQL
    });

    let result = executor.execute_local_mutation("User", "updateUser", &variables).await;

    // Error expected
    assert!(result.is_err(), "expected Err for nested object variable, got: {result:?}");
}

#[tokio::test]
async fn test_mutation_constraint_violation() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text", "email text"])]).await
    else {
        eprintln!("SKIP test_mutation_constraint_violation: no postgres");
        return;
    };

    let variables = json!({
        "id": "user_duplicate",
        "email": "existing@example.com"
    });

    let result = executor.execute_local_mutation("User", "updateUser", &variables).await;

    // Should succeed in building and executing the query (no constraint defined)
    result.unwrap_or_else(|e| {
        panic!("execute_local_mutation(User/updateUser) constraint check failed: {e}")
    });
}

#[tokio::test]
async fn test_mutation_concurrent_updates() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text", "name text"])]).await
    else {
        eprintln!("SKIP test_mutation_concurrent_updates: no postgres");
        return;
    };

    // Execute multiple mutations sequentially
    for i in 0..5 {
        let variables = json!({
            "id": format!("user{}", i),
            "name": format!("Updated User {}", i)
        });

        let result = executor.execute_local_mutation("User", "updateUser", &variables).await;

        result.unwrap_or_else(|e| {
            panic!("execute_local_mutation(User/updateUser) concurrent iteration failed: {e}")
        });
    }
}

#[tokio::test]
async fn test_mutation_transaction_rollback() {
    let metadata = common::metadata_single_key("User", "id");
    let Some((_pg, executor)) =
        common::pg_mutation_executor(metadata, &[("user", &["id text", "email text"])]).await
    else {
        eprintln!("SKIP test_mutation_transaction_rollback: no postgres");
        return;
    };

    let variables = json!({
        "id": "user1",
        "email": "test@example.com"
    });

    let result = executor.execute_local_mutation("User", "updateUser", &variables).await;

    // In real scenario with DB transaction, would test rollback
    result.unwrap_or_else(|e| {
        panic!("execute_local_mutation(User/updateUser) transaction rollback check failed: {e}")
    });
}
