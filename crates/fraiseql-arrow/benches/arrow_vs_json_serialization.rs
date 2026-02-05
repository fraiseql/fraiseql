//! Real performance benchmarks comparing Arrow Flight vs JSON query planes.
//!
//! This benchmark compares two separate data planes:
//! - JSON plane: queries `v_*` (read views) optimized for JSON serialization
//! - Arrow plane: queries `ta_*` (analytics tables) optimized for columnar format
//!
//! Both planes can represent the same logical data but may have different schemas,
//! denormalization, and storage optimizations for their respective output formats.
//!
//! Measures:
//! - Query execution latency from each plane
//! - Full round-trip time (query + serialization)
//! - Actual output size per format (JSON vs Arrow IPC)
//!
//! Requires PostgreSQL with test tables:
//! - v_users (JSON plane view) - denormalized for JSON output
//! - ta_users (Arrow plane analytics table) - columnar optimized
//!
//! Run with:
//! ```bash
//! DATABASE_URL="postgresql://localhost/postgres" cargo bench --package fraiseql-arrow --bench arrow_vs_json_serialization
//! ```

use std::{io::Cursor, sync::Arc};

use arrow::{
    datatypes::{DataType, Field, Schema},
    ipc::writer::StreamWriter,
};
use criterion::{Criterion, criterion_group, criterion_main};
use fraiseql_arrow::{
    convert::{ConvertConfig, RowToArrowConverter},
    db_convert::convert_db_rows_to_arrow,
};
use fraiseql_core::db::DatabaseAdapter;

/// Benchmark comparing JSON plane (v_users view) vs Arrow plane (ta_users table)
fn benchmark_query_planes(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup: create test tables if database is available
    let setup_result = rt.block_on(async {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string());

        match setup_test_tables(&db_url).await {
            Ok(_) => Some(db_url),
            Err(e) => {
                eprintln!("Warning: Could not setup test tables: {}", e);
                eprintln!("Benchmark requires running PostgreSQL with test data");
                None
            },
        }
    });

    if setup_result.is_none() {
        eprintln!(
            "Skipping benchmarks - PostgreSQL not available or test tables could not be created"
        );
        return;
    }

    let db_url = setup_result.unwrap();

    // Measure payload sizes once before benchmarking
    let json_size_100 = rt.block_on(query_json_plane(&db_url, "LIMIT 100")).ok().map(|b| b.len());
    let arrow_size_100 = rt.block_on(query_arrow_plane(&db_url, "LIMIT 100")).ok().map(|b| b.len());
    let json_size_1000 = rt.block_on(query_json_plane(&db_url, "LIMIT 1000")).ok().map(|b| b.len());
    let arrow_size_1000 =
        rt.block_on(query_arrow_plane(&db_url, "LIMIT 1000")).ok().map(|b| b.len());

    // Print payload sizes
    println!("\n=== Payload Sizes ===");
    println!("100 rows:");
    if let Some(json_sz) = json_size_100 {
        println!("  JSON:  {} bytes", json_sz);
    }
    if let Some(arrow_sz) = arrow_size_100 {
        println!("  Arrow: {} bytes", arrow_sz);
        if let Some(json_sz) = json_size_100 {
            let ratio = json_sz as f64 / arrow_sz as f64;
            println!("  Ratio: JSON is {:.2}x larger", ratio);
        }
    }
    println!("1000 rows:");
    if let Some(json_sz) = json_size_1000 {
        println!("  JSON:  {} bytes ({:.1} KB)", json_sz, json_sz as f64 / 1024.0);
    }
    if let Some(arrow_sz) = arrow_size_1000 {
        println!("  Arrow: {} bytes ({:.1} KB)", arrow_sz, arrow_sz as f64 / 1024.0);
        if let Some(json_sz) = json_size_1000 {
            let ratio = json_sz as f64 / arrow_sz as f64;
            println!("  Ratio: JSON is {:.2}x larger", ratio);
        }
    }
    println!("====================\n");

    let mut group = c.benchmark_group("query_plane_comparison");
    group.sample_size(10);

    // Benchmark JSON plane (v_users view) - small result set
    group.bench_function("json_plane_100_rows", |b| {
        b.to_async(&rt).iter(|| {
            let db_url = db_url.clone();
            async move { query_json_plane(&db_url, "LIMIT 100").await.unwrap_or_default() }
        });
    });

    // Benchmark Arrow plane (ta_users table) - small result set with real IPC serialization
    group.bench_function("arrow_plane_100_rows", |b| {
        b.to_async(&rt).iter(|| {
            let db_url = db_url.clone();
            async move { query_arrow_plane(&db_url, "LIMIT 100").await.unwrap_or_default() }
        });
    });

    // Benchmark JSON plane - large result set
    group.bench_function("json_plane_1000_rows", |b| {
        b.to_async(&rt).iter(|| {
            let db_url = db_url.clone();
            async move { query_json_plane(&db_url, "LIMIT 1000").await.unwrap_or_default() }
        });
    });

    // Benchmark Arrow plane - large result set with real IPC serialization
    group.bench_function("arrow_plane_1000_rows", |b| {
        b.to_async(&rt).iter(|| {
            let db_url = db_url.clone();
            async move { query_arrow_plane(&db_url, "LIMIT 1000").await.unwrap_or_default() }
        });
    });

    group.finish();
}

/// Query JSON plane (v_users) and serialize to JSON
async fn query_json_plane(
    db_url: &str,
    limit_clause: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(db_url).await?;

    let query = format!("SELECT * FROM v_users {}", limit_clause);
    let rows = adapter.execute_raw_query(&query).await?;

    // Serialize results to JSON
    let json = serde_json::to_string(&rows)?;
    Ok(json.into_bytes())
}

/// Query Arrow plane (ta_users) and serialize to Arrow IPC format
async fn query_arrow_plane(
    db_url: &str,
    limit_clause: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(db_url).await?;

    let query = format!("SELECT * FROM ta_users {}", limit_clause);
    let rows = adapter.execute_raw_query(&query).await?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Define Arrow schema for the benchmark data
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, true),
        Field::new("email", DataType::Utf8, true),
        Field::new("name", DataType::Utf8, true),
        Field::new("age", DataType::Int64, true),
        Field::new("is_active", DataType::Boolean, true),
        Field::new("created_at", DataType::Utf8, true),
        Field::new("balance", DataType::Float64, true),
        Field::new("tags", DataType::Utf8, true), // Simplified: store as JSON string
        Field::new("metadata", DataType::Utf8, true), // Store as JSON string
    ]));

    // Convert database rows to Arrow values
    let arrow_rows = convert_db_rows_to_arrow(&rows, &schema)
        .map_err(|e| format!("Failed to convert rows: {}", e))?;

    // Create converter and build RecordBatch
    let converter = RowToArrowConverter::new(schema.clone(), ConvertConfig::default());
    let batch = converter
        .convert_batch(arrow_rows)
        .map_err(|e| format!("Failed to create batch: {}", e))?;

    // Serialize to Arrow IPC format (streaming format)
    let mut buffer = Cursor::new(Vec::new());
    let mut writer = StreamWriter::try_new(&mut buffer, &batch.schema())
        .map_err(|e| format!("Failed to create writer: {}", e))?;

    writer.write(&batch).map_err(|e| format!("Failed to write batch: {}", e))?;

    writer.finish().map_err(|e| format!("Failed to finish write: {}", e))?;

    Ok(buffer.into_inner())
}

/// Setup test tables: v_users (JSON view) and ta_users (Arrow analytics table)
async fn setup_test_tables(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    use sqlx::PgPool;

    let pool = PgPool::connect(db_url).await?;

    // Create base table (tb_users)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tb_users (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL,
            name TEXT NOT NULL,
            age INT,
            is_active BOOLEAN,
            created_at TIMESTAMPTZ,
            balance NUMERIC(12,2),
            tags TEXT[],
            metadata JSONB
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create JSON plane view (v_users) - denormalized for JSON output
    sqlx::query(
        r#"
        DROP VIEW IF EXISTS v_users CASCADE
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE VIEW v_users AS
        SELECT
            id,
            email,
            name,
            age,
            is_active,
            created_at,
            balance,
            tags,
            metadata
        FROM tb_users
        "#,
    )
    .execute(&pool)
    .await?;

    // Create Arrow analytics table (ta_users) - could have different structure/partitioning
    sqlx::query("DROP TABLE IF EXISTS ta_users").execute(&pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE ta_users (
            id TEXT,
            email TEXT,
            name TEXT,
            age INT,
            is_active BOOLEAN,
            created_at TIMESTAMPTZ,
            balance NUMERIC(12,2),
            tags TEXT[],
            metadata JSONB
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Insert test data (1000 rows)
    for i in 0..1000 {
        sqlx::query(
            r#"
            INSERT INTO tb_users (id, email, name, age, is_active, created_at, balance, tags, metadata)
            VALUES ($1, $2, $3, $4, $5, NOW(), $6, $7, $8)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(format!("user-{:04}", i))
        .bind(format!("user{}@example.com", i))
        .bind(format!("User {}", i))
        .bind(20 + (i % 60) as i32)
        .bind(i % 3 != 0)
        .bind(1000.0 + (i as f64 * 10.0))
        .bind(vec!["tag1".to_string(), "tag2".to_string()])
        .bind(serde_json::json!({"index": i, "created": "2026-01-31"}))
        .execute(&pool)
        .await?;
    }

    // Sync ta_users from tb_users
    sqlx::query(
        r#"
        INSERT INTO ta_users (id, email, name, age, is_active, created_at, balance, tags, metadata)
        SELECT id, email, name, age, is_active, created_at, balance, tags, metadata FROM tb_users
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(())
}

criterion_group!(benches, benchmark_query_planes);
criterion_main!(benches);
