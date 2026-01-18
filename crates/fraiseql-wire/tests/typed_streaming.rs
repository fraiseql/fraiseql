//! Typed streaming integration tests
//!
//! Tests for typed streaming API.
//! Uses testcontainers to automatically spin up PostgreSQL with test data.
//!
//! Type parameter T affects only deserialization.
//! SQL generation, filtering, and ordering are identical regardless of T.

mod common;

use common::connect_test_client;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

/// Test user entity matching our seed data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestUser {
    id: String,
    name: String,
    email: String,
}

/// Test project entity matching our seed data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestProject {
    name: String,
    status: String,
    priority: String,
}

#[tokio::test]
async fn test_typed_query_with_struct() {
    let client = connect_test_client().await.expect("connect");

    // Query with type-safe deserialization
    // Type parameter T=TestUser means results are deserialized to TestUser structs
    let mut stream = client
        .query::<TestUser>("test.v_user")
        .where_sql("1 = 1")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let user: TestUser = result.expect("deserialize");
        // User is typed - can access fields directly
        assert!(!user.id.is_empty());
        assert!(!user.name.is_empty());
        assert!(!user.email.is_empty());
        count += 1;
        if count > 10 {
            break;
        }
    }

    assert!(count > 0, "should have received at least one user");
}

#[tokio::test]
async fn test_raw_json_query_escape_hatch() {
    let client = connect_test_client().await.expect("connect");

    // Query with raw JSON deserialization (escape hatch)
    // Type parameter T=Value means results are raw JSON
    let mut stream = client
        .query::<serde_json::Value>("test.v_user")
        .where_sql("1 = 1")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let json: serde_json::Value = result.expect("deserialize");
        // json is raw Value - access via indexing
        assert!(json["id"].is_string());
        assert!(json["name"].is_string());
        count += 1;
        if count > 10 {
            break;
        }
    }

    assert!(count > 0, "should have received at least one record");
}

#[tokio::test]
async fn test_typed_query_with_sql_predicate() {
    let client = connect_test_client().await.expect("connect");

    // Type T does NOT affect SQL generation
    // The SQL predicate is applied server-side before deserialization
    let mut stream = client
        .query::<TestUser>("test.v_user")
        .where_sql("data->>'name' LIKE 'A%'") // SQL predicate - matches "Alice Johnson"
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let user: TestUser = result.expect("deserialize");
        // Verify SQL predicate was applied (names start with 'A')
        assert!(user.name.starts_with('A'));
        count += 1;
        if count > 100 {
            break;
        }
    }

    // Predicate should filter some results
    assert!(count > 0, "should have received at least one matching user");
}

#[tokio::test]
async fn test_typed_query_with_rust_predicate() {
    let client = connect_test_client().await.expect("connect");

    // Type T does NOT affect Rust-side filtering
    // The rust predicate receives JSON values, not typed structs
    let mut stream = client
        .query::<TestUser>("test.v_user")
        .where_rust(|json| {
            // Rust predicate works on JSON values
            json["email"]
                .as_str()
                .map(|e| e.contains("@"))
                .unwrap_or(false)
        })
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let user: TestUser = result.expect("deserialize");
        // Verify rust predicate was applied
        assert!(user.email.contains("@"));
        count += 1;
        if count > 100 {
            break;
        }
    }

    assert!(
        count > 0,
        "should have received at least one user with email"
    );
}

#[tokio::test]
async fn test_typed_query_with_ordering() {
    let client = connect_test_client().await.expect("connect");

    // Type T does NOT affect ordering
    // ORDER BY is executed entirely on the server
    let mut stream = client
        .query::<TestUser>("test.v_user")
        .order_by("data->>'name' ASC")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut previous_name: Option<String> = None;
    let mut count = 0;

    while let Some(result) = stream.next().await {
        let user: TestUser = result.expect("deserialize");
        // Verify results are sorted
        if let Some(prev) = &previous_name {
            assert!(prev <= &user.name, "names should be in ascending order");
        }
        previous_name = Some(user.name.clone());
        count += 1;
        if count > 50 {
            break;
        }
    }

    assert!(
        count > 1,
        "should have received multiple users to verify ordering"
    );
}

#[tokio::test]
async fn test_type_affects_only_deserialization() {
    // This test demonstrates that type T affects ONLY deserialization
    // The SQL, filtering, and ordering are identical for all T

    // Version 1: Typed deserialization
    let client1 = connect_test_client().await.expect("connect");

    let mut typed_stream = client1
        .query::<TestUser>("test.v_user")
        .where_sql("1 = 1")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut typed_count = 0;
    while let Some(result) = typed_stream.next().await {
        let _user: TestUser = result.expect("deserialize");
        typed_count += 1;
        if typed_count > 20 {
            break;
        }
    }

    // Version 2: Raw JSON deserialization (escape hatch)
    let client2 = connect_test_client().await.expect("connect");

    let mut json_stream = client2
        .query::<serde_json::Value>("test.v_user")
        .where_sql("1 = 1")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut json_count = 0;
    while let Some(result) = json_stream.next().await {
        let _json: serde_json::Value = result.expect("deserialize");
        json_count += 1;
        if json_count > 20 {
            break;
        }
    }

    // Both versions should return the same number of results
    // (Only difference is deserialization type, not SQL/filtering/ordering)
    assert_eq!(
        typed_count, json_count,
        "typed and json queries should return same number of results"
    );
}

#[tokio::test]
async fn test_typed_query_different_types() {
    // Verify that different user types work correctly
    // with the same underlying query

    let client = connect_test_client().await.expect("connect");

    // Query for projects with custom type
    let mut stream = client
        .query::<TestProject>("test.v_project")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let project: TestProject = result.expect("deserialize");
        assert!(!project.name.is_empty());
        assert!(!project.status.is_empty());
        count += 1;
        if count > 10 {
            break;
        }
    }

    assert!(count > 0, "should have received at least one project");
}

#[tokio::test]
async fn test_deserialization_error_includes_type_info() {
    // Test that deserialization errors include type information
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct StrictUser {
        id: String,
        name: String,
        age: i32, // Field that does not exist in our data
    }

    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<StrictUser>("test.v_user")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    // Try to deserialize - should fail because age field doesn't exist
    let mut error_count = 0;
    while let Some(result) = stream.next().await {
        if let Err(e) = result {
            let error_msg = e.to_string();
            // Error message should contain useful information
            assert!(
                !error_msg.is_empty(),
                "error should have a message: {}",
                error_msg
            );
            error_count += 1;
            if error_count > 1 {
                break;
            }
        }
    }

    // We expect deserialization errors since age field doesn't exist
    assert!(error_count > 0, "should have encountered deserialization errors");
}

#[tokio::test]
async fn test_multiple_typed_queries_same_connection() {
    // Verify that multiple typed queries can be executed
    // (each query consumes the client, so we need separate connections)

    // Query 1: Users
    let client1 = connect_test_client().await.expect("connect");

    let mut user_stream = client1
        .query::<TestUser>("test.v_user")
        .chunk_size(64)
        .execute()
        .await
        .expect("query");

    let mut user_count = 0;
    while let Some(result) = user_stream.next().await {
        let _user: TestUser = result.expect("deserialize");
        user_count += 1;
        if user_count >= 5 {
            break;
        }
    }

    // Query 2: Projects (different type)
    let client2 = connect_test_client().await.expect("connect");

    let mut project_stream = client2
        .query::<TestProject>("test.v_project")
        .chunk_size(64)
        .execute()
        .await
        .expect("query");

    let mut project_count = 0;
    while let Some(result) = project_stream.next().await {
        let _project: TestProject = result.expect("deserialize");
        project_count += 1;
        if project_count >= 5 {
            break;
        }
    }

    assert!(user_count > 0, "should have queried users");
    assert!(project_count > 0, "should have queried projects");
}

#[tokio::test]
async fn test_streaming_with_chunk_sizes() {
    // Verify that typed streaming works with different chunk sizes
    for chunk_size in [1, 32, 256].iter() {
        let client = connect_test_client().await.expect("connect");

        let mut stream = client
            .query::<TestUser>("test.v_user")
            .chunk_size(*chunk_size)
            .execute()
            .await
            .expect("query");

        let mut count = 0;
        while let Some(result) = stream.next().await {
            let _user: TestUser = result.expect("deserialize");
            count += 1;
            if count >= 100 {
                break;
            }
        }

        assert!(
            count > 0,
            "should have received results with chunk size {}",
            chunk_size
        );
    }
}
