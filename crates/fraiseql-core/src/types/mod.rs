//! Type-safe identifiers and domain types for FraiseQL
//!
//! This module provides newtype wrappers for schema identifiers to enable
//! compile-time type safety and prevent accidental mixing of different identifier types.

pub mod identifiers;
pub mod sql_hints;

pub use identifiers::{FieldName, SchemaName, TableName};
pub use sql_hints::{OrderByClause, OrderDirection, SqlProjectionHint};
