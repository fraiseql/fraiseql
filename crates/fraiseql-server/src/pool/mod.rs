//! Connection pool management.
//!
//! Provides adaptive pool sizing via [`PoolSizingAdvisor`].

pub mod auto_tuner;

pub use auto_tuner::{PoolSizingAdvisor, PoolSizingRecommendation};
