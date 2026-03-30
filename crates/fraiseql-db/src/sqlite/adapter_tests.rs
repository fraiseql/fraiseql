//! Tests for the SQLite database adapter.

use fraiseql_error::FraiseQLError;
use serde_json::json;
use sqlx::Executor as _;

use super::*;
use crate::traits::{DatabaseAdapter, DirectMutationContext, DirectMutationOp, MutationStrategy};
use crate::types::DatabaseType;
use crate::where_clause::WhereClause;

/// Create an in-memory adapter and seed a `v_user` table with N rows.
async fn setup_user_table(n: usize) -> SqliteAdapter {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");
    adapter
        .pool
        .execute("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
        .await
        .expect("Failed to create v_user");
    for i in 1..=n {
        let row = format!(
            r#"INSERT INTO "v_user" (data) VALUES ('{{"id":{i},"name":"user{i}","age":{age},"active":{active},"score":{score},"deleted_at":null}}')"#,
            age = 20 + i,
            active = if i % 2 == 0 { "true" } else { "false" },
            score = i * 10,
        );
        adapter.pool.execute(row.as_str()).await.expect("Failed to insert row");
    }
    adapter
}

#[tokio::test]
async fn test_in_memory_adapter_creation() {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    let metrics = adapter.pool_metrics();
    assert!(metrics.total_connections > 0);
    assert_eq!(adapter.database_type(), DatabaseType::SQLite);
}

#[tokio::test]
async fn test_health_check() {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    adapter.health_check().await.expect("Health check failed");
}

#[tokio::test]
async fn test_raw_query() {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    // Create a test table
    sqlx::query("CREATE TABLE test_table (id INTEGER PRIMARY KEY, data TEXT)")
        .execute(&adapter.pool)
        .await
        .expect("Failed to create table");

    // Insert test data
    sqlx::query("INSERT INTO test_table (data) VALUES ('{\"name\": \"test\"}')")
        .execute(&adapter.pool)
        .await
        .expect("Failed to insert data");

    // Query the data
    let results = adapter
        .execute_raw_query("SELECT * FROM test_table")
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 1);
    assert!(results[0].contains_key("id"));
    assert!(results[0].contains_key("data"));
}

#[tokio::test]
async fn test_parameterized_limit_only() {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    // Create test table
    sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
        .execute(&adapter.pool)
        .await
        .expect("Failed to create table");

    // Insert test data
    for i in 1..=5 {
        sqlx::query(&format!(
            "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
            i, i
        ))
        .execute(&adapter.pool)
        .await
        .expect("Failed to insert data");
    }

    let results = adapter
        .execute_where_query("v_user", None, Some(2), None, None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_parameterized_offset_only() {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    // Create test table
    sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
        .execute(&adapter.pool)
        .await
        .expect("Failed to create table");

    // Insert test data
    for i in 1..=5 {
        sqlx::query(&format!(
            "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
            i, i
        ))
        .execute(&adapter.pool)
        .await
        .expect("Failed to insert data");
    }

    let results = adapter
        .execute_where_query("v_user", None, None, Some(2), None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_parameterized_limit_and_offset() {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    // Create test table
    sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
        .execute(&adapter.pool)
        .await
        .expect("Failed to create table");

    // Insert test data
    for i in 1..=5 {
        sqlx::query(&format!(
            "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
            i, i
        ))
        .execute(&adapter.pool)
        .await
        .expect("Failed to insert data");
    }

    let results = adapter
        .execute_where_query("v_user", None, Some(2), Some(1), None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_function_call_returns_unsupported_error() {
    // SQLite uses DirectSql strategy, not FunctionCall. Calling execute_function_call
    // directly still returns the default Unsupported error because SQLite doesn't
    // override it — mutations go through execute_direct_mutation instead.
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

    let err = adapter
        .execute_function_call("fn_create_user", &[json!("alice")])
        .await
        .expect_err("Expected Unsupported error");

    assert!(
        matches!(err, FraiseQLError::Unsupported { .. }),
        "Expected Unsupported error, got: {err:?}"
    );
    assert!(
        err.to_string().contains("fn_create_user"),
        "Error message should name the function"
    );
}

#[tokio::test]
async fn test_supports_mutations() {
    let adapter = SqliteAdapter::in_memory().await.unwrap();
    assert!(adapter.supports_mutations());
    assert_eq!(adapter.mutation_strategy(), MutationStrategy::DirectSql);
}

// ── Direct mutation tests ────────────────────────────────────────────────

/// Create an in-memory adapter with a `users` table for mutation testing.
async fn setup_mutation_table() -> SqliteAdapter {
    let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");
    adapter
        .pool
        .execute(
            "CREATE TABLE \"users\" (\
                pk_user INTEGER PRIMARY KEY AUTOINCREMENT, \
                name TEXT NOT NULL, \
                email TEXT NOT NULL UNIQUE, \
                tenant_id TEXT\
            )",
        )
        .await
        .expect("Failed to create users table");
    adapter
}

#[tokio::test]
async fn test_direct_mutation_insert() {
    let adapter = setup_mutation_table().await;
    let columns = vec!["name".to_string(), "email".to_string()];
    let values = vec![json!("Alice"), json!("alice@example.com")];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Insert,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &[],
        return_type:    "User",
    };

    let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["status"], json!("new"));
    assert_eq!(row["entity_type"], json!("User"));
    assert!(row["entity_id"].is_null());
    let entity = &row["entity"];
    assert_eq!(entity["name"], "Alice");
    assert_eq!(entity["email"], "alice@example.com");
    assert_eq!(entity["pk_user"], 1);
}

#[tokio::test]
async fn test_direct_mutation_insert_with_inject_params() {
    let adapter = setup_mutation_table().await;
    let columns = vec!["name".to_string(), "email".to_string()];
    let inject_columns = vec!["tenant_id".to_string()];
    let values = vec![json!("Bob"), json!("bob@example.com"), json!("tenant-42")];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Insert,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &inject_columns,
        return_type:    "User",
    };

    let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
    let entity = &rows[0]["entity"];
    assert_eq!(entity["tenant_id"], "tenant-42");
}

#[tokio::test]
async fn test_direct_mutation_update() {
    let adapter = setup_mutation_table().await;
    // Seed a row
    adapter
        .pool
        .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
        .await
        .unwrap();

    let columns = vec!["pk_user".to_string(), "name".to_string()];
    let values = vec![json!(1), json!("Alice Updated")];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Update,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &[],
        return_type:    "User",
    };

    let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
    let row = &rows[0];
    assert_eq!(row["status"], json!("updated"));
    assert_eq!(row["entity_id"], json!("1"));
    assert_eq!(row["entity"]["name"], "Alice Updated");
    assert_eq!(row["entity"]["email"], "alice@example.com");
}

#[tokio::test]
async fn test_direct_mutation_delete() {
    let adapter = setup_mutation_table().await;
    // Seed a row
    adapter
        .pool
        .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
        .await
        .unwrap();

    let columns = vec!["pk_user".to_string()];
    let values = vec![json!(1)];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Delete,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &[],
        return_type:    "User",
    };

    let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
    let row = &rows[0];
    assert_eq!(row["status"], json!("deleted"));
    assert_eq!(row["entity_id"], json!("1"));
    assert_eq!(row["entity"]["name"], "Alice");

    // Verify row is actually gone
    let remaining = adapter.execute_raw_query("SELECT * FROM \"users\"").await.unwrap();
    assert!(remaining.is_empty());
}

#[tokio::test]
async fn test_direct_mutation_delete_nonexistent_row() {
    let adapter = setup_mutation_table().await;
    let columns = vec!["pk_user".to_string()];
    let values = vec![json!(999)];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Delete,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &[],
        return_type:    "User",
    };

    let err = adapter
        .execute_direct_mutation(&ctx)
        .await
        .expect_err("Expected error for nonexistent row");
    assert!(matches!(err, FraiseQLError::Validation { .. }));
    assert!(err.to_string().contains("no rows"));
}

#[tokio::test]
async fn test_direct_mutation_constraint_violation() {
    let adapter = setup_mutation_table().await;
    adapter
        .pool
        .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
        .await
        .unwrap();

    // Insert duplicate email (UNIQUE constraint)
    let columns = vec!["name".to_string(), "email".to_string()];
    let values = vec![json!("Bob"), json!("alice@example.com")];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Insert,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &[],
        return_type:    "User",
    };

    let err = adapter
        .execute_direct_mutation(&ctx)
        .await
        .expect_err("Expected constraint violation");
    assert!(matches!(err, FraiseQLError::Database { .. }));
}

#[tokio::test]
async fn test_direct_mutation_null_handling() {
    let adapter = setup_mutation_table().await;
    adapter
        .pool
        .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
        .await
        .unwrap();

    // Update tenant_id to null
    let columns = vec!["pk_user".to_string(), "tenant_id".to_string()];
    let values = vec![json!(1), serde_json::Value::Null];

    let ctx = DirectMutationContext {
        operation:      DirectMutationOp::Update,
        table:          "users",
        columns:        &columns,
        values:         &values,
        inject_columns: &[],
        return_type:    "User",
    };

    let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
    assert!(rows[0]["entity"]["tenant_id"].is_null());
}

// ── WHERE operator matrix ─────────────────────────────────────────────────

#[tokio::test]
async fn test_where_eq_operator() {
    let adapter = setup_user_table(5).await;
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: crate::where_clause::WhereOperator::Eq,
        value:    json!("user3"),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_value()["name"], "user3");
}

#[tokio::test]
async fn test_where_neq_operator() {
    let adapter = setup_user_table(3).await;
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: crate::where_clause::WhereOperator::Neq,
        value:    json!("user1"),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_where_gt_operator() {
    let adapter = setup_user_table(5).await;
    // age = 20+i, so age > 23 → users 4 and 5
    let clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: crate::where_clause::WhereOperator::Gt,
        value:    json!(23),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_where_gte_operator() {
    let adapter = setup_user_table(5).await;
    // age >= 23 → users 3, 4, 5
    let clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: crate::where_clause::WhereOperator::Gte,
        value:    json!(23),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_where_lt_operator() {
    let adapter = setup_user_table(5).await;
    // age < 23 → users 1 and 2
    let clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: crate::where_clause::WhereOperator::Lt,
        value:    json!(23),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_where_lte_operator() {
    let adapter = setup_user_table(5).await;
    // age <= 23 → users 1, 2, 3
    let clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: crate::where_clause::WhereOperator::Lte,
        value:    json!(23),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_where_in_operator() {
    let adapter = setup_user_table(5).await;
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: crate::where_clause::WhereOperator::In,
        value:    json!(["user1", "user3", "user5"]),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_where_not_in_operator() {
    let adapter = setup_user_table(5).await;
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: crate::where_clause::WhereOperator::Nin,
        value:    json!(["user1", "user2"]),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_where_like_operator() {
    let adapter = setup_user_table(5).await;
    // name LIKE 'user%' matches all 5
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: crate::where_clause::WhereOperator::Like,
        value:    json!("user%"),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_where_is_null_operator() {
    let adapter = setup_user_table(3).await;
    // deleted_at is null for all rows (seeded as null)
    let clause = WhereClause::Field {
        path:     vec!["deleted_at".to_string()],
        operator: crate::where_clause::WhereOperator::IsNull,
        value:    json!(true),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_where_is_not_null_operator() {
    let adapter = setup_user_table(3).await;
    // deleted_at is null → IS NOT NULL returns 0 rows
    let clause = WhereClause::Field {
        path:     vec!["deleted_at".to_string()],
        operator: crate::where_clause::WhereOperator::IsNull,
        value:    json!(false),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_where_multiple_conditions_and() {
    let adapter = setup_user_table(5).await;
    // name = "user2" AND age = 22
    let clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("user2"),
        },
        WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!(22),
        },
    ]);
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_value()["name"], "user2");
}

#[tokio::test]
async fn test_where_multiple_conditions_or() {
    let adapter = setup_user_table(5).await;
    // name = "user1" OR name = "user5"
    let clause = WhereClause::Or(vec![
        WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("user1"),
        },
        WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("user5"),
        },
    ]);
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert_eq!(results.len(), 2);
}

// ── Error paths ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_empty_result_set() {
    let adapter = setup_user_table(3).await;
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: crate::where_clause::WhereOperator::Eq,
        value:    json!("nonexistent"),
    };
    let results =
        adapter.execute_where_query("v_user", Some(&clause), None, None, None).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_invalid_raw_query_returns_error() {
    let adapter = SqliteAdapter::in_memory().await.unwrap();
    let err = adapter
        .execute_raw_query("SELECT * FROM nonexistent_table_xyz")
        .await
        .expect_err("Expected database error");
    assert!(matches!(err, FraiseQLError::Database { .. }));
}

// ── Pool metrics ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_pool_metrics_when_idle() {
    let adapter = SqliteAdapter::in_memory().await.unwrap();
    let metrics = adapter.pool_metrics();
    // Idle connections should be ≤ total
    assert!(metrics.idle_connections <= metrics.total_connections);
    assert_eq!(metrics.waiting_requests, 0);
}

// ── explain_query ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_explain_query_returns_plan() {
    let adapter = setup_user_table(3).await;
    let result = adapter
        .explain_query("SELECT data FROM \"v_user\"", &[])
        .await
        .expect("explain_query should succeed");
    // EXPLAIN QUERY PLAN returns at least one step
    assert!(result.as_array().is_some_and(|a| !a.is_empty()));
}

// ── Projection ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_projection_filters_fields() {
    use crate::types::SqlProjectionHint;

    let adapter = setup_user_table(3).await;
    let projection = SqlProjectionHint {
        database:                    crate::DatabaseType::SQLite,
        projection_template:
            "json_object('name', json_extract(data, '$.name')) AS data".to_string(),
        estimated_reduction_percent: 50,
    };
    let results = adapter
        .execute_with_projection("v_user", Some(&projection), None, None, None, None)
        .await
        .expect("execute_with_projection should succeed");
    assert_eq!(results.len(), 3);
    // Only 'name' key is present; 'age' should be absent
    for row in &results {
        assert!(row.as_value().get("name").is_some());
        assert!(row.as_value().get("age").is_none());
    }
}
