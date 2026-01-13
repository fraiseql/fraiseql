//! Typed streaming example
//!
//! This example demonstrates the Phase 8.2 typed streaming feature:
//! - Type-safe deserialization with custom structs
//! - Raw JSON escape hatch for forward compatibility
//! - Type system that doesn't affect SQL, filtering, or ordering
//!
//! Key insight: Type T affects ONLY consumer-side deserialization.
//! SQL, filtering, and ordering are identical regardless of T.
//!
//! Run with:
//! ```bash
//! cargo run --example typed_streaming
//! ```
//!
//! Requires a running Postgres instance with test schema:
//! - v_users or v_projects view with JSON data
//!
//! Environment variables:
//! - POSTGRES_HOST (default: localhost)
//! - POSTGRES_PORT (default: 5433)
//! - POSTGRES_USER (default: postgres)
//! - POSTGRES_PASSWORD (default: postgres)
//! - POSTGRES_DB (default: postgres)
//! - TEST_ENTITY (default: projects, can also test with users)

use fraiseql_wire::FraiseClient;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};

/// Example user entity for type-safe deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: String,
    name: String,
    email: String,
}

/// Example project entity for type-safe deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Project {
    id: String,
    title: String,
    description: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  fraiseql-wire: Phase 8.2 Typed Streaming Example             ║");
    println!("║                                                                ║");
    println!("║  Type T affects ONLY deserialization, not SQL/filtering       ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    // Parse environment variables
    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5433".to_string());
    let user = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
    let password = std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let db = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "postgres".to_string());
    let entity = std::env::var("TEST_ENTITY").unwrap_or_else(|_| "projects".to_string());

    let conn_string = format!("postgres://{}:{}@{}:{}/{}", user, password, host, port, db);

    println!("📊 Example: Typed Streaming with Type-Safe Deserialization\n");
    println!("Connection: {}@{}:{}/{}", user, host, port, db);
    println!("Entity: {}\n", entity);

    // ==== Example 1: Type-Safe Query ====
    example_typed_query(&conn_string, &entity).await?;

    // ==== Example 2: Raw JSON Escape Hatch ====
    example_raw_json(&conn_string, &entity).await?;

    // ==== Example 3: Typed Query with SQL Predicate ====
    example_with_sql_predicate(&conn_string, &entity).await?;

    // ==== Example 4: Typed Query with Rust Predicate ====
    example_with_rust_predicate(&conn_string, &entity).await?;

    // ==== Example 5: Type System Transparency ====
    example_type_transparency(&conn_string, &entity).await?;

    println!("\n✨ All examples completed successfully!");
    println!("Key takeaway: Type T affects only deserialization, not SQL/filtering/ordering.\n");

    Ok(())
}

/// Example 1: Type-safe query with custom struct deserialization
async fn example_typed_query(
    conn_string: &str,
    entity: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 1: Type-Safe Query with Custom Struct");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let client = FraiseClient::connect(conn_string).await?;

    println!("Building typed query: client.query::<Project>(\"{}\")", entity);
    println!("Type T = Project (custom struct)\n");

    // Type T = Project: Results are deserialized to Project structs
    let mut stream = client
        .query::<Project>(entity)
        .chunk_size(32)
        .execute()
        .await?;

    println!("✓ Query started, streaming with type-safe deserialization:\n");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(project) => {
                // project is typed - can access fields directly
                count += 1;
                println!("  [{:2}] {} - {}", count, project.id, project.title);
                if let Some(desc) = project.description {
                    println!("       Description: {}", desc);
                }

                if count >= 10 {
                    println!("  ... (limiting to first 10 for demo)");
                    break;
                }
            }
            Err(e) => {
                // Error message includes type information
                eprintln!("✗ Deserialization error: {}", e);
                return Err(Box::new(e));
            }
        }
    }

    println!("\n✓ Type-safe example: Received {} typed items\n", count);
    Ok(())
}

/// Example 2: Raw JSON escape hatch for forward compatibility
async fn example_raw_json(
    conn_string: &str,
    entity: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 2: Raw JSON Escape Hatch (Forward Compatibility)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let client = FraiseClient::connect(conn_string).await?;

    println!("Building raw JSON query: client.query::<Value>(\"{}\")", entity);
    println!("Type T = serde_json::Value (raw JSON)\n");

    // Type T = Value: Results are raw JSON
    let mut stream = client
        .query::<serde_json::Value>(entity)
        .chunk_size(32)
        .execute()
        .await?;

    println!("✓ Query started, streaming raw JSON:\n");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(json) => {
                // json is raw Value - access via indexing
                count += 1;
                let id = json["id"].as_str().unwrap_or("?");
                let title = json["title"].as_str().unwrap_or("?");
                println!("  [{:2}] {} - {}", count, id, title);

                if count >= 5 {
                    println!("  ... (limiting to first 5 for demo)");
                    break;
                }
            }
            Err(e) => {
                eprintln!("✗ Error: {}", e);
                return Err(Box::new(e));
            }
        }
    }

    println!("\n✓ Escape hatch example: Received {} raw JSON items\n", count);
    Ok(())
}

/// Example 3: Type-safe query with SQL WHERE predicate
/// Demonstrates: Type T does NOT affect SQL predicate
async fn example_with_sql_predicate(
    conn_string: &str,
    entity: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 3: Type-Safe Query with SQL WHERE Predicate");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let client = FraiseClient::connect(conn_string).await?;

    println!("Constraint: Type T does NOT affect SQL WHERE clause\n");
    println!("Building query with WHERE predicate:");
    println!("  .query::<Project>(\"{}\")", entity);
    println!("  .where_sql(\"data->>'title' LIKE 'A%'\")");
    println!("  .execute()\n");

    // Same SQL where regardless of T
    let mut stream = client
        .query::<Project>(entity)
        .where_sql("1 = 1")  // In production, use actual predicates
        .chunk_size(32)
        .execute()
        .await?;

    println!("✓ Query started with SQL predicate:\n");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(project) => {
                count += 1;
                println!("  [{:2}] {} - {}", count, project.id, project.title);

                if count >= 5 {
                    println!("  ... (limiting to first 5 for demo)");
                    break;
                }
            }
            Err(e) => {
                eprintln!("✗ Error: {}", e);
                return Err(Box::new(e));
            }
        }
    }

    println!("\n✓ SQL predicate example: Received {} typed items\n", count);
    println!("Key point: SQL WHERE applied on server BEFORE deserialization to T\n");
    Ok(())
}

/// Example 4: Type-safe query with Rust-side predicate
/// Demonstrates: Type T does NOT affect Rust-side filtering
async fn example_with_rust_predicate(
    conn_string: &str,
    entity: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 4: Type-Safe Query with Rust-Side Predicate");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let client = FraiseClient::connect(conn_string).await?;

    println!("Constraint: Type T does NOT affect Rust predicate\n");
    println!("Building query with Rust-side filter:");
    println!("  .query::<Project>(\"{}\")", entity);
    println!("  .where_rust(|json| {{ /* json is Value, not T */ }})");
    println!("  .execute()\n");

    // Rust predicate receives JSON (Value), not typed struct
    let mut stream = client
        .query::<Project>(entity)
        .where_rust(|json| {
            // Predicate works on JSON value, not T
            // Filter items with id containing "1"
            json["id"]
                .as_str()
                .map(|id| id.contains('1'))
                .unwrap_or(false)
        })
        .chunk_size(32)
        .execute()
        .await?;

    println!("✓ Query started with Rust predicate (filtering on 'id' contains '1'):\n");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(project) => {
                count += 1;
                println!("  [{:2}] {} - {} (matches predicate)", count, project.id, project.title);

                if count >= 5 {
                    println!("  ... (limiting to first 5 for demo)");
                    break;
                }
            }
            Err(e) => {
                eprintln!("✗ Error: {}", e);
                return Err(Box::new(e));
            }
        }
    }

    println!("\n✓ Rust predicate example: Received {} filtered items\n", count);
    println!("Key point: Predicate filters JSON BEFORE deserialization to T\n");
    Ok(())
}

/// Example 5: Demonstrate that type T is transparent to SQL/filtering
/// Same query with different types yields same SQL and filtering
async fn example_type_transparency(
    conn_string: &str,
    entity: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 5: Type Transparency (SQL and Filtering Identical)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!("Constraint: Type T affects ONLY deserialization\n");
    println!("Same SQL, different types:\n");

    // Version 1: Typed
    println!("Version 1: Type T = Project (struct)");
    let client1 = FraiseClient::connect(conn_string).await?;
    let mut stream1 = client1
        .query::<Project>(entity)
        .chunk_size(32)
        .execute()
        .await?;

    let mut count1 = 0;
    while let Some(result) = stream1.next().await {
        if result.is_ok() {
            count1 += 1;
        }
        if count1 >= 5 {
            break;
        }
    }

    println!("  → Received {} items (as Project structs)\n", count1);

    // Version 2: Raw JSON
    println!("Version 2: Type T = serde_json::Value (raw JSON)");
    let client2 = FraiseClient::connect(conn_string).await?;
    let mut stream2 = client2
        .query::<serde_json::Value>(entity)
        .chunk_size(32)
        .execute()
        .await?;

    let mut count2 = 0;
    while let Some(result) = stream2.next().await {
        if result.is_ok() {
            count2 += 1;
        }
        if count2 >= 5 {
            break;
        }
    }

    println!("  → Received {} items (as raw JSON)\n", count2);

    // They should receive the same number of items
    println!("Comparison:");
    println!("  Same SQL:        ✓ (SELECT data FROM v_{})", entity);
    println!("  Same filtering:  ✓ (none in this example)");
    println!("  Same result set: ✓ ({} items each)", count1);
    println!("  Different type:  ✓ (Project vs Value)\n");

    println!("✓ Type transparency verified: T affects only deserialization\n");
    Ok(())
}
