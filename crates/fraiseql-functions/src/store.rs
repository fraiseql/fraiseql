//! Function storage traits and implementations.
//!
//! Provides the [`FunctionStore`] trait for persisting function deployments,
//! and [`InMemoryFunctionStore`] for testing.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use bytes::Bytes;
use tokio::sync::RwLock;

use crate::types::RuntimeType;

/// A stored function record containing bytecode and metadata.
#[derive(Debug, Clone)]
pub struct FunctionRecord {
    /// Unique name for this function.
    pub name: String,
    /// Compiled bytecode or source text.
    pub bytecode: Bytes,
    /// Which runtime executes this module.
    pub runtime: RuntimeType,
}

/// Trait for function deployment storage.
///
/// Object-safe for use as `Arc<dyn FunctionStore>` in server contexts.
pub trait FunctionStore: Send + Sync {
    /// Retrieve a function by name.
    fn get_function(
        &self,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<Option<FunctionRecord>>> + Send + '_>>;

    /// Store (deploy) a function.
    fn store_function(
        &self,
        name: &str,
        runtime: RuntimeType,
        bytecode: Bytes,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<()>> + Send + '_>>;
}

/// In-memory function store for testing.
#[derive(Debug, Clone, Default)]
pub struct InMemoryFunctionStore {
    functions: Arc<RwLock<HashMap<String, FunctionRecord>>>,
}

impl InMemoryFunctionStore {
    /// Create a new empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl FunctionStore for InMemoryFunctionStore {
    fn get_function(
        &self,
        name: &str,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<Option<FunctionRecord>>> + Send + '_>>
    {
        let name = name.to_string();
        Box::pin(async move { Ok(self.functions.read().await.get(&name).cloned()) })
    }

    fn store_function(
        &self,
        name: &str,
        runtime: RuntimeType,
        bytecode: Bytes,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<()>> + Send + '_>> {
        let name = name.to_string();
        Box::pin(async move {
            self.functions.write().await.insert(
                name.clone(),
                FunctionRecord {
                    name,
                    bytecode,
                    runtime,
                },
            );
            Ok(())
        })
    }
}
