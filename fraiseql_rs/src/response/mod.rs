//! Zero-copy result streaming and response building.
//!
//! This module implements memory-efficient streaming from PostgreSQL
//! directly to HTTP response bytes, eliminating intermediate buffering.

pub mod builder;
pub mod json_transform;
pub mod streaming;

pub use builder::ResponseBuilder;
pub use json_transform::{to_camel_case, transform_row_keys, transform_jsonb_field};
pub use streaming::{ResponseStream, ChunkedWriter};