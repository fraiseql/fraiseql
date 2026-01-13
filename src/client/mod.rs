//! High-level client API
//!
//! This module provides the user-facing API for fraiseql-wire.

mod fraise_client;
mod connection_string;
mod query_builder;

pub use fraise_client::FraiseClient;
pub use query_builder::QueryBuilder;
