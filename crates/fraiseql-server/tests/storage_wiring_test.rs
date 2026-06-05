//! Integration test for the `[storage.<name>]` → `StorageState` boot wiring
//! (#334).
//!
//! Verifies that `build_storage_state` connects to PostgreSQL, ensures the
//! object-metadata table exists, constructs the backend, and assembles a
//! `StorageState` carrying the configured logical bucket. The happy path
//! requires PostgreSQL (`DATABASE_URL`) and skips gracefully when it is unset;
//! the "no storage configured" path makes no database connection and always
//! runs.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stderr)] // Reason: test code.

use std::collections::HashMap;

use fraiseql_server::{
    ServerConfig,
    server_config::{StorageSectionConfig, build_storage_state},
};

#[tokio::test]
async fn build_storage_state_returns_none_without_config() {
    // No `[storage.<name>]` section: resolves to None before any DB connection,
    // so this runs without PostgreSQL.
    let config = ServerConfig::default();
    let state = build_storage_state(&config).await.expect("no storage section must not error");
    assert!(state.is_none(), "no [storage] section should resolve to None");
}

#[tokio::test]
async fn build_storage_state_wires_local_backend_from_config() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!(
            "skipping build_storage_state_wires_local_backend_from_config: DATABASE_URL unset"
        );
        return;
    };

    let tmp = tempfile::tempdir().expect("create temp dir for local backend");

    let mut storage = HashMap::new();
    storage.insert(
        "assets".to_string(),
        StorageSectionConfig {
            backend:            "local".to_string(),
            path:               Some(tmp.path().to_string_lossy().into_owned()),
            bucket:             None,
            region:             None,
            endpoint:           None,
            project_id:         None,
            account_name:       None,
            access:             Some("public_read".to_string()),
            max_object_bytes:   Some(1024),
            allowed_mime_types: None,
            serve_inline:       None,
        },
    );

    let config = ServerConfig {
        database_url,
        storage,
        ..ServerConfig::default()
    };

    let state = build_storage_state(&config)
        .await
        .expect("build_storage_state should succeed against PostgreSQL")
        .expect("a configured [storage.assets] section should yield a StorageState");

    assert!(
        state.buckets.contains_key("assets"),
        "logical bucket 'assets' should be present"
    );
    let bucket = state.buckets.get("assets").expect("bucket present");
    assert_eq!(bucket.name, "assets");
    assert_eq!(bucket.max_object_bytes, Some(1024));
}
