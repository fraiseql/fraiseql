//! Handlers for the `/api/v1` operator API: admin operations, query intelligence and
//! federation discovery.
//!
//! Every route is mounted by `server::routing` behind the admin bearer gate, or behind its
//! own authentication; none is mounted here. All return structured JSON responses.

pub mod admin;
/// The operator SQL console (#962) — compiled in only with the `admin-sql`
/// feature, because it executes SQL the operator typed.
#[cfg(feature = "admin-sql")]
pub mod admin_sql;
pub mod design;
#[cfg(feature = "federation")]
pub mod federation;
pub mod metadata;
pub mod openapi;
pub mod query;
pub mod query_stats;
pub mod schema;
pub mod storage_policies;
pub mod tenant_admin;
pub mod types;
pub mod usage;

// Re-export commonly used types
pub use types::{ApiError, ApiResponse};

#[cfg(test)]
mod tests;
