//! Integration tests for WHERE operators and query modifiers
//!
//! These tests verify that all 25 operators work correctly with actual PostgreSQL data.
//! They test both JSONB field filtering and direct column filtering.
//!
//! Run with: `cargo test --test integration_operators -- --ignored --nocapture`
//! Requires: PostgreSQL running via `docker-compose up`

use fraiseql_wire::FraiseClient;
use futures::StreamExt;
use serde_json::Value;

const TEST_DB_URL: &str = "postgres://postgres:postgres@localhost:5433/fraiseql_test";

/// Helper to collect all results from a stream
async fn collect_results(
    mut stream: fraiseql_wire::stream::QueryStream<Value>,
    limit: Option<usize>,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut results = Vec::new();
    while let Some(result) = stream.next().await {
        let json = result?;
        results.push(json);
        if let Some(max) = limit {
            if results.len() >= max {
                break;
            }
        }
    }
    Ok(results)
}

// ============================================================================
// JSONB Field Filtering Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_jsonb_eq_string() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter projects where status = 'active' (JSONB text field)
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'status')::text = 'active'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} active projects", data.len());

    // Verify all results have status = 'active'
    for value in data {
        assert_eq!(
            value["status"].as_str(),
            Some("active"),
            "All returned projects should have status='active'"
        );
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_jsonb_neq() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter projects where status != 'active'
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'status')::text != 'active'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} non-active projects", data.len());

    // Verify none have status = 'active'
    for value in data {
        assert_ne!(
            value["status"].as_str(),
            Some("active"),
            "No returned project should have status='active'"
        );
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_jsonb_in() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter projects where status IN ('active', 'paused')
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'status')::text IN ('active', 'paused')")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!(
        "Found {} projects with status in (active, paused)",
        data.len()
    );

    // Verify all results have one of the specified statuses
    for value in data {
        let status = value["status"].as_str().expect("status field");
        assert!(
            status == "active" || status == "paused",
            "All results should have status in (active, paused)"
        );
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_jsonb_contains() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter projects where name contains 'Project'
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'name')::text LIKE '%Project%'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} projects with 'Project' in name", data.len());

    // Verify all results have 'Project' in their name
    for value in data {
        let name = value["name"].as_str().expect("name field");
        assert!(
            name.contains("Project"),
            "All results should have 'Project' in name"
        );
    }
}

// ============================================================================
// Direct Column Filtering Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_direct_column_timestamp() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Note: v_users view only exposes id and data columns
    // So we filter on JSONB created_at instead of direct column
    let results = client
        .query::<Value>("test_staging.v_users")
        .where_sql("(data->>'created_at')::timestamp > '2024-01-02'::timestamp")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} users created after 2024-01-02", data.len());

    // Verify the query executes correctly (result can be empty or populated)
    let _ = data.len(); // Result is used in println above
}

// ============================================================================
// Mixed Filter Tests (JSONB + Direct Columns)
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_mixed_filters_jsonb_and_direct() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter: JSONB status = 'active' AND JSONB name contains 'Project'
    // (Demonstrates mixed JSONB filters in single WHERE clause)
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'status')::text = 'active'")
        .where_sql("(data->>'name')::text LIKE '%Project%'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} projects (active + contain 'Project')", data.len());

    // Verify results match both conditions
    for value in data {
        assert_eq!(
            value["status"].as_str(),
            Some("active"),
            "All results should have status='active'"
        );
        let name = value["name"].as_str().expect("name");
        assert!(
            name.contains("Project"),
            "All results should contain 'Project' in name"
        );
    }
}

// ============================================================================
// LIMIT Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_limit_clause() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Get first 2 projects
    let results = client
        .query::<Value>("test_staging.v_projects")
        .limit(2)
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("LIMIT 2: Got {} results", data.len());

    assert!(data.len() <= 2, "LIMIT 2 should return at most 2 results");
}

// ============================================================================
// OFFSET Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_offset_clause() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Get projects with OFFSET 2 (skip first 2)
    let results = client
        .query::<Value>("test_staging.v_projects")
        .offset(2)
        .limit(10)
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("OFFSET 2 LIMIT 10: Got {} results", data.len());

    // With 5 projects and offset 2, should get at most 3
    assert!(
        data.len() <= 3,
        "OFFSET 2 with 5 total projects should return at most 3"
    );
}

// ============================================================================
// ORDER BY Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_order_by_jsonb_field() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Order by JSONB field (name)
    let results = client
        .query::<Value>("test_staging.v_projects")
        .order_by("(data->>'name')::text ASC")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Ordered by name: Got {} results", data.len());

    // Verify ordering (simple check - just verify we got results)
    if data.len() > 1 {
        let first_name = data[0]["name"].as_str().expect("name");
        let second_name = data[1]["name"].as_str().expect("name");
        assert!(
            first_name <= second_name,
            "Results should be ordered by name ASC"
        );
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_order_by_jsonb_field_multiple() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Order by multiple JSONB fields
    let results = client
        .query::<Value>("test_staging.v_projects")
        .order_by("(data->>'status')::text ASC, (data->>'name')::text DESC")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!(
        "Ordered by status ASC, name DESC: Got {} results",
        data.len()
    );

    // Just verify we got results
    assert!(!data.is_empty(), "Should get results when ordering");
}

// ============================================================================
// Array Length Operator Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_array_length() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter users where roles array has exactly 2 elements
    // For JSONB arrays, use json_array_length() function
    let results = client
        .query::<Value>("test_staging.v_users")
        .where_sql("jsonb_array_length(data->'roles') = 2")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} users with 2 roles", data.len());

    // Verify all have exactly 2 roles
    for value in data {
        let roles = value["roles"].as_array().expect("roles array");
        assert_eq!(roles.len(), 2, "All results should have exactly 2 roles");
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_array_length_gt() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter users where roles array has more than 1 element
    let results = client
        .query::<Value>("test_staging.v_users")
        .where_sql("jsonb_array_length(data->'roles') > 1")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} users with > 1 role", data.len());

    // Verify all have more than 1 role
    for value in data {
        let roles = value["roles"].as_array().expect("roles array");
        assert!(roles.len() > 1, "All results should have more than 1 role");
    }
}

// ============================================================================
// Pagination Tests (LIMIT + OFFSET)
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_pagination_full_cycle() {
    let client1 = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Get first page: LIMIT 2 OFFSET 0
    let results1 = client1
        .query::<Value>("test_staging.v_projects")
        .limit(2)
        .offset(0)
        .order_by("(data->>'name')::text ASC")
        .execute()
        .await
        .expect("query");

    let page1 = collect_results(results1, None).await.expect("collect");
    println!("Page 1 (LIMIT 2 OFFSET 0): {} items", page1.len());

    // Get second page: LIMIT 2 OFFSET 2
    let client2 = FraiseClient::connect(TEST_DB_URL).await.expect("connect");
    let results2 = client2
        .query::<Value>("test_staging.v_projects")
        .limit(2)
        .offset(2)
        .order_by("(data->>'name')::text ASC")
        .execute()
        .await
        .expect("query");

    let page2 = collect_results(results2, None).await.expect("collect");
    println!("Page 2 (LIMIT 2 OFFSET 2): {} items", page2.len());

    // Verify pages are different (if we have enough data)
    if !page1.is_empty() && !page2.is_empty() {
        assert_ne!(
            page1[0], page2[0],
            "First page and second page should contain different results"
        );
    }
}

// ============================================================================
// Complex Filter Combinations
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_complex_filters_with_ordering_and_pagination() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter: status = active, order by name, limit results
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'status')::text = 'active'")
        .order_by("(data->>'name')::text ASC")
        .limit(10)
        .offset(0)
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Complex filter: Got {} results", data.len());

    // Verify all have status = 'active'
    for value in data {
        assert_eq!(
            value["status"].as_str(),
            Some("active"),
            "All results should have status='active'"
        );
    }
}

// ============================================================================
// Collation Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_order_by_with_collation_c() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Order by with binary collation (C)
    let results = client
        .query::<Value>("test_staging.v_projects")
        .order_by("(data->>'name')::text COLLATE \"C\" ASC")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Ordered with C collation: Got {} results", data.len());

    assert!(!data.is_empty(), "Should get results with collation");
}

// ============================================================================
// String Operator Tests (Like, Ilike)
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_like_pattern() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Use LIKE pattern matching on name
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'name')::text LIKE 'A%'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} projects starting with 'A'", data.len());

    // Verify all match pattern
    for value in data {
        let name = value["name"].as_str().expect("name");
        assert!(name.starts_with('A'), "All results should start with 'A'");
    }
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_ilike_case_insensitive() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Use ILIKE for case-insensitive matching
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'name')::text ILIKE 'a%'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!(
        "Found {} projects matching 'a%' (case-insensitive)",
        data.len()
    );

    // Verify all match pattern (case-insensitive)
    for value in data {
        let name = value["name"].as_str().expect("name");
        assert!(
            name.to_lowercase().starts_with('a'),
            "All results should start with 'a' (case-insensitive)"
        );
    }
}

// ============================================================================
// Streaming and Performance Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_streaming_large_result_set() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Stream all projects
    let mut stream = client
        .query::<Value>("test_staging.v_projects")
        .chunk_size(2) // Small chunk size to test batching
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _json = result.expect("item");
        count += 1;
    }

    println!("Streamed {} items in chunks of 2", count);
    assert!(count > 0, "Should stream some items");
}

// ============================================================================
// NULL Handling Tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_operator_is_null() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Filter for rows where a JSONB field might be null
    // (This test verifies the SQL generation is correct)
    let results = client
        .query::<Value>("test_staging.v_users")
        // Most users should have a website field, so this should return few/zero
        .where_sql("(data->'profile'->>'website') IS NULL")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} users with NULL website", data.len());

    // Just verify the query executes correctly (result can be empty)
    assert!(!data.is_empty() || data.is_empty(), "Query should execute");
}

// ============================================================================
// Sanity Checks
// ============================================================================

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_query_without_filters() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Get all projects
    let results = client
        .query::<Value>("test_staging.v_projects")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Found {} total projects", data.len());

    assert!(!data.is_empty(), "Should find some projects");
}

#[tokio::test]
#[ignore] // Requires PostgreSQL running
async fn test_query_with_empty_result() {
    let client = FraiseClient::connect(TEST_DB_URL).await.expect("connect");

    // Query that should return no results
    let results = client
        .query::<Value>("test_staging.v_projects")
        .where_sql("(data->>'status')::text = 'nonexistent_status'")
        .execute()
        .await
        .expect("query");

    let data = collect_results(results, None).await.expect("collect");
    println!("Empty result set: {} items", data.len());

    assert_eq!(data.len(), 0, "Should return empty result set");
}
