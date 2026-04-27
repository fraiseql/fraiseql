#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

use super::storage_migration_sql;

/// Execute multi-statement DDL by splitting on semicolons.
async fn execute_ddl(pool: &PgPool, ddl: &str) {
    for stmt in ddl.split(';') {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await.unwrap();
        }
    }
}

#[test]
fn test_migration_ddl_is_valid_sql() {
    let ddl = storage_migration_sql();

    // Must contain the table name
    assert!(
        ddl.contains("_fraiseql_storage_objects"),
        "DDL must create _fraiseql_storage_objects table"
    );

    // Must use IF NOT EXISTS for idempotency
    assert!(
        ddl.contains("IF NOT EXISTS"),
        "DDL must use IF NOT EXISTS"
    );

    // Must have the required columns matching StorageMetadataRow
    for col in [
        "pk_storage_object",
        "bucket",
        "key",
        "content_type",
        "size_bytes",
        "etag",
        "owner_id",
        "created_at",
        "updated_at",
    ] {
        assert!(ddl.contains(col), "DDL must contain column: {col}");
    }

    // Must have indexes
    assert!(
        ddl.contains("idx_storage_objects_bucket_key"),
        "DDL must create bucket+key index"
    );
    assert!(
        ddl.contains("idx_storage_objects_owner"),
        "DDL must create owner index"
    );

    // Must follow Trinity pattern for primary key
    assert!(
        ddl.contains("GENERATED ALWAYS AS IDENTITY"),
        "pk must use GENERATED ALWAYS AS IDENTITY (Trinity pattern)"
    );
}

#[tokio::test]
async fn test_migration_creates_table() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();

    let ddl = storage_migration_sql();
    execute_ddl(&pool, ddl).await;

    // Verify table exists by querying pg_class
    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_class WHERE relname = '_fraiseql_storage_objects'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(exists, "table _fraiseql_storage_objects must exist after migration");
}

#[tokio::test]
async fn test_migration_is_idempotent() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();

    let ddl = storage_migration_sql();

    // Run twice — second run must not error
    execute_ddl(&pool, ddl).await;
    execute_ddl(&pool, ddl).await;
}
