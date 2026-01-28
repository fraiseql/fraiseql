//! Federation support modules for Apollo Federation v2.
//!
//! This module provides observability and health checking for federated GraphQL queries.

pub mod health_checker;

pub use health_checker::{RollingErrorWindow, SubgraphConfig, SubgraphHealthChecker, SubgraphHealthStatus};
