//! Arrow Flight server integration for high-performance columnar data transfer.
//!
//! This module provides Arrow Flight server integration that enables clients
//! to fetch data via the Apache Arrow Flight protocol for efficient,
//! high-performance data retrieval.
//!
//! # Features
//!
//! - Arrow Flight gRPC service
//! - Support for ta_* materialized tables
//! - Schema registry with pre-compiled Arrow schemas
//! - Zero-copy row-to-Arrow conversion
//!
//! # Usage
//!
//! This module is only available when the "arrow" feature is enabled.

#[cfg(feature = "arrow")]
pub mod database_adapter;

#[cfg(feature = "arrow")]
pub use database_adapter::FlightDatabaseAdapter;
