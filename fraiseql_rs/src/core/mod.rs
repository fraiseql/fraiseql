//! Core transformation engine for zero-copy GraphQL JSON processing
//!
//! This module provides the foundation for ultra-fast JSON transformations
//! with minimal memory allocations and SIMD optimizations.

pub mod arena;
pub mod camel;
pub mod transform;

// Re-export key types for convenience
pub use arena::Arena;
pub use camel::snake_to_camel; // New unified API
pub use transform::{ByteBuf, TransformConfig, ZeroCopyTransformer};
