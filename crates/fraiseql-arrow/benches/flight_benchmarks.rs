//! Performance benchmarks for Arrow Flight integration.
//!
//! Run with:
//! ```bash
//! cargo bench --package fraiseql-arrow --bench flight_benchmarks
//! ```

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use fraiseql_arrow::db::DatabaseAdapter as ArrowDatabaseAdapter;
use fraiseql_core::db::DatabaseAdapter;
use sqlx::postgres::PgPoolOptions;

/// Test database for benchmarks
struct BenchDb {
    #[allow(dead_code)]
    pool: sqlx::PgPool,
}

impl BenchDb {
    async fn setup() -> Result<Self, Box<dyn std::error::Error>> {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string());

        let pool = PgPoolOptions::new().max_connections(5).connect(&db_url).await?;

        // Create test tables if they don't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ta_users (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                source_updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS ta_orders (
                id TEXT PRIMARY KEY,
                total NUMERIC(12, 2) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                customer_name TEXT NOT NULL,
                source_updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Ensure test data exists
        sqlx::query(
            r#"
            INSERT INTO ta_users (id, name, email, created_at)
            VALUES
                ('bench-user-1', 'Alice Johnson', 'alice@example.com', NOW()),
                ('bench-user-2', 'Bob Smith', 'bob@example.com', NOW()),
                ('bench-user-3', 'Charlie Brown', 'charlie@example.com', NOW()),
                ('bench-user-4', 'Diana Prince', 'diana@example.com', NOW()),
                ('bench-user-5', 'Eve Wilson', 'eve@example.com', NOW())
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO ta_orders (id, total, created_at, customer_name)
            VALUES
                ('bench-order-1', 99.99, NOW(), 'Alice Johnson'),
                ('bench-order-2', 149.99, NOW(), 'Bob Smith'),
                ('bench-order-3', 199.99, NOW(), 'Charlie Brown'),
                ('bench-order-4', 299.99, NOW(), 'Diana Prince'),
                ('bench-order-5', 399.99, NOW(), 'Eve Wilson')
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(BenchDb { pool })
    }

    fn connection_string(&self) -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/postgres".to_string())
    }
}

fn adapter_initialization(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("adapter_init_postgres", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box("postgresql://localhost/postgres".to_string());
            fraiseql_core::db::postgres::PostgresAdapter::new(&db_url).await
        });
    });
}

fn query_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let bench_db =
        rt.block_on(async { BenchDb::setup().await.expect("Failed to setup benchmark database") });

    let mut group = c.benchmark_group("query_latency");

    // Small result set (1 row)
    group.bench_function("query_1_row", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            adapter
                .execute_raw_query(black_box("SELECT * FROM ta_users WHERE id = 'bench-user-1'"))
                .await
        });
    });

    // Medium result set (5 rows)
    group.bench_function("query_5_rows", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            adapter.execute_raw_query(black_box("SELECT * FROM ta_users LIMIT 5")).await
        });
    });

    // Full table scan (5 rows, all columns)
    group.bench_function("query_full_table_scan", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            adapter.execute_raw_query(black_box("SELECT * FROM ta_users")).await
        });
    });

    // Query with WHERE clause
    group.bench_function("query_with_filter", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            adapter
                .execute_raw_query(black_box(
                    "SELECT id, name FROM ta_users WHERE id LIKE 'bench-user%'",
                ))
                .await
        });
    });

    // Query with ORDER BY
    group.bench_function("query_with_order_by", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            adapter.execute_raw_query(black_box("SELECT * FROM ta_users ORDER BY id")).await
        });
    });

    group.finish();
}

fn flight_adapter_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let bench_db =
        rt.block_on(async { BenchDb::setup().await.expect("Failed to setup benchmark database") });

    let mut group = c.benchmark_group("flight_adapter");

    // Flight adapter wrapping overhead
    group.bench_function("adapter_wrapping_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            let _flight_adapter = fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter);
        });
    });

    // Flight adapter query execution
    group.bench_function("flight_query_5_rows", |b| {
        b.to_async(&rt).iter(|| async {
            let db_url = black_box(bench_db.connection_string());
            let pg_adapter = fraiseql_core::db::postgres::PostgresAdapter::new(&db_url)
                .await
                .expect("Failed to create adapter");
            let flight_adapter = fraiseql_server::arrow::FlightDatabaseAdapter::new(pg_adapter);
            flight_adapter
                .execute_raw_query(black_box("SELECT * FROM ta_users LIMIT 5"))
                .await
        });
    });

    group.finish();
}

fn row_conversion_latency(c: &mut Criterion) {
    c.bench_function("json_to_arrow_5_rows", |b| {
        b.iter(|| {
            // Simulate 5 rows of JSON data
            let rows = vec![
                serde_json::json!({
                    "id": "user-1",
                    "name": "Alice",
                    "email": "alice@example.com",
                    "created_at": "2026-01-31T00:00:00Z"
                }),
                serde_json::json!({
                    "id": "user-2",
                    "name": "Bob",
                    "email": "bob@example.com",
                    "created_at": "2026-01-31T00:00:00Z"
                }),
                serde_json::json!({
                    "id": "user-3",
                    "name": "Charlie",
                    "email": "charlie@example.com",
                    "created_at": "2026-01-31T00:00:00Z"
                }),
                serde_json::json!({
                    "id": "user-4",
                    "name": "Diana",
                    "email": "diana@example.com",
                    "created_at": "2026-01-31T00:00:00Z"
                }),
                serde_json::json!({
                    "id": "user-5",
                    "name": "Eve",
                    "email": "eve@example.com",
                    "created_at": "2026-01-31T00:00:00Z"
                }),
            ];

            // In real code, these would be converted to Arrow RecordBatches
            black_box(rows).len()
        });
    });
}

criterion_group!(
    benches,
    adapter_initialization,
    query_latency,
    flight_adapter_latency,
    row_conversion_latency
);
criterion_main!(benches);
