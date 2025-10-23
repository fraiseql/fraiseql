//! JSON manipulation utilities for zero-copy operations
//!
//! This module provides high-performance JSON processing components:
//! - Streaming JSON parser (zero-copy)
//! - Direct byte buffer JSON writer
//! - SIMD-optimized JSON escaping

pub mod escape;
