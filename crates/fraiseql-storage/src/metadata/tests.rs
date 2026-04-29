#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions

use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

use super::{NewStorageObject, StorageMetadataRepo};

/// DDL for the metadata table, used by tests and later exposed as migration SQL.
const CREATE_TABLE_DDL: &str = r"
CREATE TABLE IF NOT EXISTS _fraiseql_storage_objects (
    pk_storage_object BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    bucket            TEXT        NOT NULL,
    key               TEXT        NOT NULL,
    content_type      TEXT        NOT NULL,
    size_bytes        BIGINT      NOT NULL,
    etag              TEXT,
    owner_id          TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (bucket, key)
);
";

/// Start a throw-away PostgreSQL container and return a pool with the schema created.
async fn setup_pg() -> (PgPool, impl std::any::Any) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query(CREATE_TABLE_DDL)
        .execute(&pool)
        .await
        .unwrap();
    (pool, container)
}

fn sample_object(bucket: &str, key: &str) -> NewStorageObject {
    NewStorageObject {
        bucket: bucket.to_string(),
        key: key.to_string(),
        content_type: "image/png".to_string(),
        size_bytes: 1024,
        etag: Some("abc123".to_string()),
        owner_id: Some("user-1".to_string()),
    }
}

#[tokio::test]
async fn test_insert_metadata_returns_id() {
    let (pool, _container) = setup_pg().await;
    let repo = StorageMetadataRepo::new(pool);

    let id = repo.insert(&sample_object("avatars", "photo.png")).await.unwrap();
    assert!(id > 0, "insert should return a positive primary key");
}

#[tokio::test]
async fn test_get_metadata_by_bucket_and_key() {
    let (pool, _container) = setup_pg().await;
    let repo = StorageMetadataRepo::new(pool);

    let obj = sample_object("avatars", "photo.png");
    repo.insert(&obj).await.unwrap();

    let row = repo.get("avatars", "photo.png").await.unwrap();
    let row = row.expect("should find the inserted row");
    assert_eq!(row.bucket, "avatars");
    assert_eq!(row.key, "photo.png");
    assert_eq!(row.content_type, "image/png");
    assert_eq!(row.size_bytes, 1024);
    assert_eq!(row.etag.as_deref(), Some("abc123"));
    assert_eq!(row.owner_id.as_deref(), Some("user-1"));
}

#[tokio::test]
async fn test_delete_metadata_removes_row() {
    let (pool, _container) = setup_pg().await;
    let repo = StorageMetadataRepo::new(pool);

    repo.insert(&sample_object("avatars", "photo.png")).await.unwrap();

    let deleted = repo.delete("avatars", "photo.png").await.unwrap();
    assert!(deleted, "delete should return true for existing row");

    let row = repo.get("avatars", "photo.png").await.unwrap();
    assert!(row.is_none(), "row should be gone after delete");

    let deleted_again = repo.delete("avatars", "photo.png").await.unwrap();
    assert!(!deleted_again, "second delete should return false");
}

#[tokio::test]
async fn test_list_metadata_with_prefix() {
    let (pool, _container) = setup_pg().await;
    let repo = StorageMetadataRepo::new(pool);

    // Insert 5 objects: 2 match prefix "docs/", 3 don't
    for key in ["docs/readme.md", "docs/guide.pdf", "images/a.png", "images/b.png", "root.txt"] {
        repo.insert(&sample_object("bucket", key)).await.unwrap();
    }

    let rows = repo.list("bucket", Some("docs/"), 100, 0).await.unwrap();
    assert_eq!(rows.len(), 2, "only docs/ prefix objects should match");
    assert!(rows.iter().all(|r| r.key.starts_with("docs/")));
}

#[tokio::test]
async fn test_list_metadata_pagination() {
    let (pool, _container) = setup_pg().await;
    let repo = StorageMetadataRepo::new(pool);

    // Insert 10 objects
    for i in 0..10 {
        repo.insert(&sample_object("bucket", &format!("file-{i:02}.dat")))
            .await
            .unwrap();
    }

    let page = repo.list("bucket", None, 3, 3).await.unwrap();
    assert_eq!(page.len(), 3, "limit=3 should return 3 rows");
    // With key ordering, offset=3 should skip the first 3
    assert_eq!(page[0].key, "file-03.dat");
    assert_eq!(page[2].key, "file-05.dat");
}

#[tokio::test]
async fn test_upsert_metadata_on_reupload() {
    let (pool, _container) = setup_pg().await;
    let repo = StorageMetadataRepo::new(pool);

    let obj = sample_object("avatars", "photo.png");
    let id1 = repo.upsert(&obj).await.unwrap();

    // Re-upload with different size and etag
    let updated = NewStorageObject {
        size_bytes: 2048,
        etag: Some("def456".to_string()),
        ..obj
    };
    let id2 = repo.upsert(&updated).await.unwrap();

    assert_eq!(id1, id2, "upsert should return the same pk");

    let row = repo.get("avatars", "photo.png").await.unwrap().unwrap();
    assert_eq!(row.size_bytes, 2048, "size should be updated");
    assert_eq!(row.etag.as_deref(), Some("def456"), "etag should be updated");
    assert!(row.updated_at >= row.created_at, "updated_at should be >= created_at");
}
