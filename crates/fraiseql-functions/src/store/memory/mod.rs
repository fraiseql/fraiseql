//! In-memory function store for unit tests and local development.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
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

#[async_trait]
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

        let version = guard.next_version.entry(name.to_string()).or_insert(0);
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
mod tests;
