//! Image transformation and caching module.
//!
//! Provides on-the-fly image resizing, format conversion, and caching capabilities
//! when the `transforms` feature is enabled.

#[cfg(feature = "transforms")]
pub mod cache;
#[cfg(feature = "transforms")]
pub mod transformer;

#[cfg(test)]
mod tests;

#[cfg(feature = "transforms")]
pub use transformer::{ImageTransformer, OutputFormat, TransformOutput, TransformParams};

/// Re-export for convenience when transforms feature is enabled
#[cfg(feature = "transforms")]
pub use cache::TransformCache;
