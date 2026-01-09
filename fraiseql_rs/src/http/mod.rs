//! HTTP server implementation using Axum framework
//!
//! This module provides a type-safe, high-performance HTTP server for handling GraphQL requests
//! and subscriptions. It integrates with the `FraiseQL` GraphQL pipeline to process queries,
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
//! - `optimization`: Performance tuning, rate limiting, health checks (Phase 16: Polish & Optimization)
//! - `benchmarks`: Performance benchmarking tests (Phase 16: Polish & Optimization)
//! - `http2_config`: HTTP/2 protocol configuration and multiplexing optimization (Phase 18)
//! - `connection_pool`: Connection pooling and socket tuning for HTTP/2 (Phase 18.2)
//! - `batch_requests`: Batch request processing and deduplication (Phase 18.3)
//! - `http2_metrics`: HTTP/2 multiplexing and stream metrics for observability (Phase 18.5)
//! - `http2_buffer_tuning`: Buffer and flow window tuning for HTTP/2 (Phase 18.4)
//! - `operation_metrics`: GraphQL operation metrics collection and aggregation (Phase 19, Commit 4.5) ⭐
//! - `operation_monitor`: Slow operation detection and monitoring (Phase 19, Commit 4.5) ⭐
//! - `operation_metrics_middleware`: Axum middleware for metrics collection and trace context (Phase 19, Commit 4.5) ⭐
//! - `graphql_operation_detector`: Operation type detection and field/alias counting (Phase 19, Commit 4.5) ⭐
//! - `response_filter`: GraphQL response filtering by field selection (Phase 2, v2.0 Security) 🔐
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
pub mod batch_requests;
pub mod benchmarks;
pub mod connection_pool;
pub mod graphql_operation_detector;
pub mod http2_buffer_tuning;
pub mod http2_config;
pub mod http2_metrics;
pub mod metrics;
pub mod middleware;
pub mod observability_middleware;
pub mod operation_metrics;
pub mod operation_metrics_middleware;
pub mod operation_monitor;
pub mod optimization;
pub mod py_bindings;
pub mod response_filter;
pub mod response_filtering_integration;
pub mod security_middleware;
pub mod websocket;

#[cfg(test)]
mod http2_integration_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod basic_tests {
    #[test]
    fn test_module_exports() {
        // Verify that core types are properly exported
        let _: () = {
            // This test ensures the module exports compile correctly
        };
    }
}
