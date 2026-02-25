//! Federation support modules for Apollo Federation v2.
//!
//! This module provides observability, health checking, and circuit breaking
//! for federated GraphQL queries.

pub mod circuit_breaker;
pub mod health_checker;

pub use circuit_breaker::FederationCircuitBreakerManager;
pub use health_checker::{
    RollingErrorWindow, SubgraphConfig, SubgraphHealthChecker, SubgraphHealthStatus,
};
