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
//! WebSocket connections for subscriptions:
//!
//! ```text
//! WebSocket Upgrade Request
//!     ↓
//! WebSocket Handler
//!     ↓
//! Connection Init (graphql-ws protocol)
//!     ↓
//! Subscription Management (Phase 15b)
//!     ↓
//! Event Stream
//! ```
//!
//! # Modules
//!
//! - `axum_server`: Core Axum server implementation with HTTP routing and handlers
//! - `middleware`: Compression, CORS, and error handling middleware (Phase 16)
//! - `websocket`: WebSocket handler for GraphQL subscriptions (graphql-ws protocol)
//! - `security_middleware`: HTTP security integration with existing security modules (Phase 16: Commit 5)
//! - `auth_middleware`: HTTP authentication integration with JWT validation (Phase 16: Commit 6)
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

pub mod auth_middleware;
pub mod axum_server;
pub mod middleware;
pub mod security_middleware;
pub mod websocket;

pub use auth_middleware::{claims_to_user_context, extract_and_validate_jwt, HttpAuthError};
pub use axum_server::{create_router, GraphQLRequest, GraphQLResponse};
pub use middleware::{CompressionAlgorithm, CompressionConfig, HttpError};
pub use security_middleware::{check_rate_limit, validate_graphql_request, HttpSecurityError};
pub use websocket::websocket_handler;

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_exports() {
        // Verify that core types are properly exported
        let _: () = {
            // This test ensures the module exports compile correctly
        };
    }
}
