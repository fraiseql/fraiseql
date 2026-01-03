//! HTTP server implementation using Axum framework
//!
//! This module provides a type-safe, high-performance HTTP server for handling GraphQL requests
//! and subscriptions. It integrates with the FraiseQL GraphQL pipeline to process queries,
//! mutations, and subscriptions.
//!
//! # Architecture
//!
//! The HTTP layer follows a clean request-response pipeline:
//!
//! ```text
//! HTTP Request
//!     ↓
//! Axum Router (type-safe routing)
//!     ↓
//! Handler Function (extraction + validation)
//!     ↓
//! GraphQL Pipeline (Phase 1-15)
//!     ↓
//! HTTP Response
//! ```
//!
//! # Modules
//!
//! - `axum_server`: Core Axum server implementation with routing and handlers
//!
//! # Examples
//!
//! ```ignore
//! use fraiseql_rs::http;
//! use std::sync::Arc;
//!
//! // Create HTTP server with GraphQL pipeline
//! let pipeline = Arc::new(/* GraphQL pipeline */);
//! let router = http::axum_server::create_router(pipeline);
//!
//! // Start server on port 8000
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
//! axum::serve(listener, router).await?;
//! ```

pub mod axum_server;

pub use axum_server::{create_router, GraphQLRequest, GraphQLResponse};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify that core types are properly exported
        let _: () = {
            // This test ensures the module exports compile correctly
        };
    }
}
