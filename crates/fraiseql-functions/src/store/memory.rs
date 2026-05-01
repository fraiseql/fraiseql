//! In-memory function store for unit tests and local development.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fraiseql_error::{FraiseQLError, Result};

use crate::types::RuntimeType;

use super::{FunctionRecord, FunctionStatus, FunctionStore};

/// In-memory function store backed by a `HashMap` behind a `Mutex`.
///
/// Thread-safe via an `Arc<Mutex<...>>` interior; suitable for unit tests
/// and local development scenarios that do not require persistence.
#[derive(Debug, Clone)]
pub struct InMemoryFunctionStore {
    inner: Arc<Mutex<StoreInner>>,
}

#[derive(Debug, Default)]
struct StoreInner {
    /// Latest record per function name.
    records: HashMap<String, FunctionRecord>,
    /// Next pk to assign.
    next_pk: i64,
    /// Next version per function name.
    next_version: HashMap<String, i32>,
}

impl Default for InMemoryFunctionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryFunctionStore {
    /// Create a new empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StoreInner {
                records: HashMap::new(),
                next_pk: 1,
                next_version: HashMap::new(),
            })),
        }
    }
}

impl FunctionStore for InMemoryFunctionStore {
    async fn store_function(
        &self,
        name: &str,
        runtime: RuntimeType,
        bytecode: bytes::Bytes,
    ) -> Result<FunctionRecord> {
        let mut guard = self.inner.lock().map_err(|_| FraiseQLError::Validation {
            message: "function store mutex poisoned".to_string(),
            path: None,
        })?;

        let pk = guard.next_pk;
        guard.next_pk += 1;

        let version = guard
            .next_version
            .entry(name.to_string())
            .or_insert(0);
        *version += 1;
        let ver = *version;

        let record = FunctionRecord {
            pk_function: pk,
            name: name.to_string(),
            runtime,
            bytecode,
            version: ver,
            deployed_at: chrono::Utc::now(),
            status: FunctionStatus::Active,
        };

        // Deactivate the previous record for this name (keep only the latest)
        guard.records.insert(name.to_string(), record.clone());
        Ok(record)
    }

    async fn get_function(&self, name: &str) -> Result<Option<FunctionRecord>> {
        let guard = self.inner.lock().map_err(|_| FraiseQLError::Validation {
            message: "function store mutex poisoned".to_string(),
            path: None,
        })?;

        let record = guard
            .records
            .get(name)
            .filter(|r| r.status == FunctionStatus::Active)
            .cloned();

        Ok(record)
    }

    async fn list_functions(&self) -> Result<Vec<FunctionRecord>> {
        let guard = self.inner.lock().map_err(|_| FraiseQLError::Validation {
            message: "function store mutex poisoned".to_string(),
            path: None,
        })?;

        let mut records: Vec<FunctionRecord> = guard
            .records
            .values()
            .filter(|r| r.status == FunctionStatus::Active)
            .cloned()
            .collect();

        // Stable ordering by name for deterministic test assertions
        records.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(records)
    }

    async fn delete_function(&self, name: &str) -> Result<bool> {
        let mut guard = self.inner.lock().map_err(|_| FraiseQLError::Validation {
            message: "function store mutex poisoned".to_string(),
            path: None,
        })?;

        let found = match guard
            .records
            .get_mut(name)
            .filter(|r| r.status == FunctionStatus::Active)
        {
            Some(r) => {
                r.status = FunctionStatus::Inactive;
                true
            }
            None => false,
        };

        Ok(found)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests {
    use bytes::Bytes;

    use crate::types::RuntimeType;

    use super::*;

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

        store
            .store_function("fn_a", RuntimeType::Wasm, sample_bytes())
            .await
            .unwrap();
        store
            .store_function("fn_b", RuntimeType::Deno, sample_bytes())
            .await
            .unwrap();

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

        store
            .store_function("keep", RuntimeType::Wasm, sample_bytes())
            .await
            .unwrap();
        store
            .store_function("gone", RuntimeType::Wasm, sample_bytes())
            .await
            .unwrap();

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
        assert_eq!(
            FunctionStatus::parse("active"),
            Some(FunctionStatus::Active)
        );
        assert_eq!(
            FunctionStatus::parse("inactive"),
            Some(FunctionStatus::Inactive)
        );
        assert_eq!(FunctionStatus::parse("unknown"), None);
    }
}
