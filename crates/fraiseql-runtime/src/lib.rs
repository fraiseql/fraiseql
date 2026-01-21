//! FraiseQL Runtime - Configuration and execution runtime for FraiseQL endpoints
//!
//! This crate provides the runtime infrastructure for executing FraiseQL endpoints,
//! including configuration parsing, HTTP server integration, middleware, and lifecycle management.

pub mod config;
pub mod lifecycle;
pub mod middleware;
pub mod observability;
pub mod resilience;
pub mod server;
pub mod state;
pub mod testing;

// Re-export commonly used types
pub use config::RuntimeConfig;
pub use lifecycle::shutdown::{ShutdownCoordinator, ShutdownConfig};
pub use server::RuntimeServer;
pub use state::AppState;
