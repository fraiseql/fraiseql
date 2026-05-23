#![allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions

use bytes::Bytes;

use super::*;
use crate::types::RuntimeType;

fn sample_bytes() -> Bytes {
    Bytes::from_static(b"\x00asm\x01\x00\x00\x00")
}

#[tokio::test]
async fn test_store_and_retrieve_function() {
    let store = InMemoryFunctionStore::new();

    let record = store
        .store_function("my_func", RuntimeType::Wasm, sample_bytes())
        .await
        .unwrap();

    assert_eq!(record.name, "my_func");
    assert_eq!(record.runtime, RuntimeType::Wasm);
    assert_eq!(record.version, 1);
    assert_eq!(record.status, FunctionStatus::Active);

    let retrieved = store.get_function("my_func").await.unwrap();
    assert!(retrieved.is_some());
    let r = retrieved.unwrap();
    assert_eq!(r.name, "my_func");
    assert_eq!(r.version, 1);
}

#[tokio::test]
async fn test_redeploy_bumps_version() {
    let store = InMemoryFunctionStore::new();

    store
        .store_function("versioned", RuntimeType::Wasm, sample_bytes())
        .await
        .unwrap();

    let v2 = store
        .store_function("versioned", RuntimeType::Wasm, sample_bytes())
        .await
        .unwrap();

    assert_eq!(v2.version, 2);

    // Only latest is returned
    let got = store.get_function("versioned").await.unwrap().unwrap();
    assert_eq!(got.version, 2);
}

#[tokio::test]
async fn test_list_functions_returns_active_only() {
    let store = InMemoryFunctionStore::new();

    store.store_function("fn_a", RuntimeType::Wasm, sample_bytes()).await.unwrap();
    store.store_function("fn_b", RuntimeType::Deno, sample_bytes()).await.unwrap();

    let list = store.list_functions().await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "fn_a");
    assert_eq!(list[1].name, "fn_b");
}

#[tokio::test]
async fn test_delete_function_returns_true_when_found() {
    let store = InMemoryFunctionStore::new();

    store
        .store_function("to_delete", RuntimeType::Wasm, sample_bytes())
        .await
        .unwrap();

    let deleted = store.delete_function("to_delete").await.unwrap();
    assert!(deleted);

    // No longer active
    let got = store.get_function("to_delete").await.unwrap();
    assert!(got.is_none());
}

#[tokio::test]
async fn test_delete_function_returns_false_when_not_found() {
    let store = InMemoryFunctionStore::new();

    let deleted = store.delete_function("ghost").await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn test_list_excludes_deleted_functions() {
    let store = InMemoryFunctionStore::new();

    store.store_function("keep", RuntimeType::Wasm, sample_bytes()).await.unwrap();
    store.store_function("gone", RuntimeType::Wasm, sample_bytes()).await.unwrap();

    store.delete_function("gone").await.unwrap();

    let list = store.list_functions().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "keep");
}

#[tokio::test]
async fn test_get_missing_function_returns_none() {
    let store = InMemoryFunctionStore::new();
    let result = store.get_function("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_function_status_roundtrip() {
    assert_eq!(FunctionStatus::Active.as_str(), "active");
    assert_eq!(FunctionStatus::Inactive.as_str(), "inactive");
    assert_eq!(FunctionStatus::parse("active"), Some(FunctionStatus::Active));
    assert_eq!(FunctionStatus::parse("inactive"), Some(FunctionStatus::Inactive));
    assert_eq!(FunctionStatus::parse("unknown"), None);
}
