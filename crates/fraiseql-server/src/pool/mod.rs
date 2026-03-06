//! Connection pool management.
//!
//! Provides adaptive pool sizing via [`PoolAutoTuner`].

pub mod auto_tuner;

pub use auto_tuner::{PoolAutoTuner, PoolTuningDecision};
