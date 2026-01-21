//! Middleware components for the FraiseQL runtime.
//!
//! This module contains middleware for:
//! - Admission control (backpressure) - Phase 1 ✅
//! - Request tracking and tracing - Phase 2
//! - Rate limiting - Phase 2
//! - CORS - Phase 1 (via tower-http)
//! - Compression - Phase 1 (via tower-http)
//! - Timeout handling - Phase 1 (via tower-http)

pub mod admission;
