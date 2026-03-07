//! Type-safe identifiers and domain types for FraiseQL
//!
//! This module provides newtype wrappers for schema identifiers to enable
//! compile-time type safety and prevent accidental mixing of different identifier types.

// sql_hints types are in fraiseql-db; re-export for backward compat
pub use fraiseql_db::types::{OrderByClause, OrderDirection, SqlProjectionHint};
