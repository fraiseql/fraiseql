//! Function deployment store for persisting and retrieving compiled function artifacts.
//!
//! The store holds versioned function bytecode so the server can load functions at
//! runtime without re-reading from disk or a remote registry.
//!
//! # Implementations
//!
//! - [`InMemoryFunctionStore`] — for unit tests and local development
//! - `PgFunctionStore` — PostgreSQL-backed persistent store (feature: `host-live`)

pub mod memory;

use async_trait::async_trait;
use fraiseql_error::Result;

use crate::types::RuntimeType;

/// Deployment status of a function version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FunctionStatus {
    /// This version is the active (serving) version.
    Active,
    /// This version has been superseded by a newer deploy.
    Inactive,
}

impl FunctionStatus {
    /// Serialize as the DB column string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }

    /// Parse from a DB column string.
    ///
    /// Returns `None` if the value is unrecognized.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "inactive" => Some(Self::Inactive),
            _ => None,
        }
    }
}

/// A stored function record representing one deployed version.
#[derive(Debug, Clone)]
pub struct FunctionRecord {
    /// Trinity-pattern primary key (`pk_function`).
    pub pk_function: i64,
    /// Unique function name (scoped per tenant).
    pub name:        String,
    /// Which runtime executes this function.
    pub runtime:     RuntimeType,
    /// Compiled bytecode or source text.
    pub bytecode:    bytes::Bytes,
    /// Monotonically increasing deploy version (1-based).
    pub version:     i32,
    /// When this version was deployed.
    pub deployed_at: chrono::DateTime<chrono::Utc>,
    /// Whether this version is active.
    pub status:      FunctionStatus,
}

/// Trait for function deployment stores.
///
/// Implementors persist and retrieve versioned function bytecode.
///
/// The `#[async_trait]` attribute generates a dyn-compatible vtable so that
/// `Box<dyn FunctionStore>` / `Arc<dyn FunctionStore>` work for dynamic dispatch.
#[async_trait]
pub trait FunctionStore: Send + Sync {
    /// Store a new version of a function, bumping its version number.
    ///
    /// If a function with `name` already exists, a new version is created
    /// and previous versions are marked `inactive`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails.
    async fn store_function(
        &self,
        name: &str,
        runtime: RuntimeType,
        bytecode: bytes::Bytes,
    ) -> Result<FunctionRecord>;

    /// Retrieve the latest active version of a function by name.
    ///
    /// Returns `Ok(None)` if no active function with that name exists.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the read fails.
    async fn get_function(&self, name: &str) -> Result<Option<FunctionRecord>>;

    /// List all active function records.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the read fails.
    async fn list_functions(&self) -> Result<Vec<FunctionRecord>>;

    /// Delete (deactivate) all versions of a function by name.
    ///
    /// Returns `true` if at least one active record was found and deactivated,
    /// `false` if no active function with that name existed.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails.
    async fn delete_function(&self, name: &str) -> Result<bool>;
}
