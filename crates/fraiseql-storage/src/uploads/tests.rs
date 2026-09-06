//! Tests for the resumable-upload session repository, against real PostgreSQL.

#![allow(clippy::unwrap_used)] // Reason: test module
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use chrono::Utc;
use sqlx::PgPool;

use super::{NewUploadSession, UploadSessionRepo};

async fn repo() -> Option<(UploadSessionRepo, PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    crate::migrations::run_storage_migration(&pool).await.unwrap();
    sqlx::query("TRUNCATE _fraiseql_storage_uploads").execute(&pool).await.unwrap();
    Some((UploadSessionRepo::new(pool.clone()), pool, svc))
}

fn new_session(bucket: &str, key: &str) -> NewUploadSession {
    NewUploadSession {
        bucket:              bucket.to_string(),
        key:                 key.to_string(),
        content_type:        "application/octet-stream".to_string(),
        declared_bytes:      10,
        owner_id:            Some("owner-a".to_string()),
        pk_storage_object:   1,
        created_reservation: true,
        backend_state:       serde_json::json!({}),
        expires_at:          Utc::now() + chrono::Duration::hours(1),
    }
}

#[tokio::test]
async fn create_get_roundtrip_and_key_uniqueness() {
    let Some((repo, _pool, _svc)) = repo().await else {
        eprintln!("SKIP create_get_roundtrip_and_key_uniqueness: no postgres");
        return;
    };
    let id = repo.create(&new_session("b", "k.bin")).await.unwrap().expect("created");
    let session = repo.get(id).await.unwrap().expect("exists");
    assert_eq!(session.bucket, "b");
    assert_eq!(session.key, "k.bin");
    assert_eq!(session.received_bytes, 0);
    assert_eq!(session.declared_bytes, 10);
    assert!(session.created_reservation);

    // A second in-flight session for the same (bucket, key) is refused — the
    // caller answers 409, it does not clobber the first upload.
    assert!(repo.create(&new_session("b", "k.bin")).await.unwrap().is_none());
    // A different key is independent.
    assert!(repo.create(&new_session("b", "other.bin")).await.unwrap().is_some());
}

#[tokio::test]
async fn advance_is_pinned_to_the_proven_offset() {
    let Some((repo, _pool, _svc)) = repo().await else {
        eprintln!("SKIP advance_is_pinned_to_the_proven_offset: no postgres");
        return;
    };
    let id = repo.create(&new_session("b", "advance.bin")).await.unwrap().expect("created");

    let state = serde_json::json!({ "etags": ["a"] });
    assert!(repo.advance(id, 0, 5, &state).await.unwrap(), "first append at offset 0 wins");
    // A concurrent append that also proved offset 0 must lose.
    assert!(!repo.advance(id, 0, 5, &state).await.unwrap(), "stale offset is refused");

    let session = repo.get(id).await.unwrap().expect("exists");
    assert_eq!(session.received_bytes, 5);
    assert_eq!(session.backend_state, state);
}

#[tokio::test]
async fn delete_removes_the_session() {
    let Some((repo, _pool, _svc)) = repo().await else {
        eprintln!("SKIP delete_removes_the_session: no postgres");
        return;
    };
    let id = repo.create(&new_session("b", "gone.bin")).await.unwrap().expect("created");
    assert!(repo.delete(id).await.unwrap());
    assert!(repo.get(id).await.unwrap().is_none());
    assert!(!repo.delete(id).await.unwrap(), "double delete reports nothing removed");
}
