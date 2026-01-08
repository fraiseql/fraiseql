//! Zero-copy result streaming and response building.
//!
//! This module implements memory-efficient streaming from `PostgreSQL`
//! directly to HTTP response bytes, eliminating intermediate buffering.
//!
//! **Security Feature**: Unified field filtering across all response types ensures
//! only requested fields are returned, preventing unauthorized field exposure from
//! cached responses (APQ, subscriptions, etc.).

pub mod builder;
pub mod field_filter;
pub mod json_transform;
pub mod streaming;

pub use json_transform::transform_row_keys;
pub use streaming::{ChunkedWriter, ResponseStream};
