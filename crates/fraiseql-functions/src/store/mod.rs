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

use crate::types::RuntimeType;
use fraiseql_error::Result;

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
            FunctionStatus::Active => "active",
            FunctionStatus::Inactive => "inactive",
        }
    }

    /// Parse from a DB column string.
    ///
    /// Returns `None` if the value is unrecognized.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(FunctionStatus::Active),
            "inactive" => Some(FunctionStatus::Inactive),
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
    pub name: String,
    /// Which runtime executes this function.
    pub runtime: RuntimeType,
    /// Compiled bytecode or source text.
    pub bytecode: bytes::Bytes,
    /// Monotonically increasing deploy version (1-based).
    pub version: i32,
    /// When this version was deployed.
    pub deployed_at: chrono::DateTime<chrono::Utc>,
    /// Whether this version is active.
    pub status: FunctionStatus,
}

/// Trait for function deployment stores.
///
/// Implementors persist and retrieve versioned function bytecode.
///
/// The `#[trait_variant::make]` macro generates `SendFunctionStore` which is
/// object-safe for `Box<dyn SendFunctionStore>` dynamic dispatch.
#[allow(clippy::trait_duplication_in_bounds)]
#[trait_variant::make(SendFunctionStore: Send)]
pub trait FunctionStore: Send + Sync {
    /// Store a new version of a function, bumping its version number.
    ///
    /// If a function with `name` already exists, a new version is created
    /// and previous versions are marked `inactive`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails.
    fn store_function(
        &self,
        name: &str,
        runtime: RuntimeType,
        bytecode: bytes::Bytes,
    ) -> impl std::future::Future<Output = Result<FunctionRecord>> + Send;

    /// Retrieve the latest active version of a function by name.
    ///
    /// Returns `Ok(None)` if no active function with that name exists.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the read fails.
    fn get_function(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<Option<FunctionRecord>>> + Send;

    /// List all active function records.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the read fails.
    fn list_functions(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<FunctionRecord>>> + Send;

    /// Delete (deactivate) all versions of a function by name.
    ///
    /// Returns `true` if at least one active record was found and deactivated,
    /// `false` if no active function with that name existed.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the write fails.
    fn delete_function(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<bool>> + Send;
}
