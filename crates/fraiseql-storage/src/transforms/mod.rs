//! Image transformation and caching module.
//!
//! Provides on-the-fly image resizing, format conversion, and caching capabilities
//! when the `transforms` feature is enabled.

#[cfg(feature = "transforms")]
pub mod cache;
#[cfg(feature = "transforms")]
pub mod ops;
#[cfg(feature = "transforms-retarget")]
pub mod retarget;
#[cfg(feature = "transforms")]
pub mod text;
#[cfg(feature = "transforms")]
pub mod transformer;

#[cfg(test)]
pub(crate) mod tests;

/// Re-export for convenience when transforms feature is enabled
#[cfg(feature = "transforms")]
pub use cache::TransformCache;
#[cfg(feature = "transforms")]
pub use ops::{CropSpec, Gravity, ResizeMode, Watermark, parse_colour};
#[cfg(feature = "transforms")]
pub use transformer::{ImageTransformer, OutputFormat, TransformOutput, TransformParams};
