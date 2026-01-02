//! APQ storage backends
//!
//! Implementations of the `ApqStorage` trait for different backends:
//! - Memory: In-process LRU cache (single instance)
//! - PostgreSQL: Distributed persistent storage (multi-instance)

pub mod memory;
pub mod postgresql;

pub use memory::MemoryApqStorage;
pub use postgresql::PostgresApqStorage;
