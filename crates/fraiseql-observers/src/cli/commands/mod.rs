//! Command implementations for observer CLI.

pub mod debug_event;
pub mod dlq;
pub mod status;
pub mod validate_config;

#[cfg(feature = "metrics")]
pub mod metrics;
