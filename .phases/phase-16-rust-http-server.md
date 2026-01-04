# Phase 16: Native Rust HTTP Server

**Status**: Planning
**Target Version**: FraiseQL v2.0
**Total Effort**: 2-3 weeks (80-120 hours)
**Commits**: 12-15
**Lines of Code**: ~3,000 Rust + ~500 Python

---

## 🎯 Executive Summary

Replace the Python HTTP layer (FastAPI/Starlette) with a native Rust HTTP server while maintaining 100% backward compatibility with the Python API. Users continue writing pure Python code—the HTTP server swap is an implementation detail.

### Why Phase 16?

**Current bottleneck**: Python HTTP layer (FastAPI/uvicorn)
- Rust pipeline: 7-12ms (Phases 1-15)
- Python HTTP: 5-10ms overhead
- Total: 12-22ms end-to-end

**After Phase 16**:
- Rust HTTP: <1ms overhead
- Rust pipeline: 7-12ms
- Total: 7-12ms end-to-end
- **Improvement: 1.5-3x faster** (elimination of Python HTTP layer)

### The Promise

```python
# User code: UNCHANGED
import fraiseql

@fraiseql.type
class User:
    id: int
    name: str

app = fraiseql.create_fraiseql_app(schema=schema)

# Internal: HTTP server now in Rust
# External: Identical API, better performance
```

---

## 📊 Current Architecture

### Today (Phases 1-15)

```
Request from client
    ↓
[uvicorn - Python ASGI server]
    ↓
[FastAPI - Python HTTP router]
    ↓
[Python request parsing/validation]
    ↓
[Rust GraphQL Pipeline] ← Does 95% of the work
    ├── Query parsing
    ├── SQL generation
    ├── Cache lookup
    ├── Auth/RBAC/Security
    ├── Query execution
    └── Response building
    ↓
[Python JSON encoder]
    ↓
[uvicorn - Python ASGI response handler]
    ↓
Response to client
```

### After Phase 16

```
Request from client
    ↓
[Rust HTTP Server] ← New: Replaces uvicorn + FastAPI
    ├── Accept connection
    ├── Parse HTTP request
    └── Route to /graphql
    ↓
[Rust Request Handler]
    ├── Extract JSON body
    ├── Parse request parameters
    └── Build GraphQL request
    ↓
[Rust GraphQL Pipeline] ← Unchanged from Phases 1-15
    ├── Query parsing
    ├── SQL generation
    ├── Cache lookup
    ├── Auth/RBAC/Security
    ├── Query execution
    └── Response building (returns bytes)
    ↓
[Rust HTTP Response Handler]
    ├── Set status code
    ├── Set headers
    └── Send bytes directly
    ↓
Response to client
```

**Key difference**: No Python in the request path. Rust all the way.

---

## 🏗️ Architecture Design

### Layer 1: HTTP Server Core (Rust)

**Purpose**: Accept TCP connections and route HTTP requests

```rust
// fraiseql_rs/src/http/
├── server.rs          // Tokio HTTP listener + TCP accept loop
├── routing.rs         // Route matching (/graphql, /graphql/subscriptions, etc.)
├── request.rs         // Parse HTTP request body
├── response.rs        // Build HTTP response with status/headers
└── mod.rs             // Public exports
```

**Responsibilities**:
- Listen on configured host:port (default 0.0.0.0:8000)
- Accept TCP connections
- Route HTTP requests to appropriate handlers
- Handle graceful shutdown

**Crate dependencies**:
- `tokio` - Already available (Phase 15b)
- `http` - HTTP types (status, headers, methods)
- `hyper` or `axum` - HTTP server frameworks

### Layer 2: GraphQL Request Handler (Rust)

**Purpose**: Parse GraphQL requests and delegate to Rust pipeline

```rust
// fraiseql_rs/src/http/
├── graphql_handler.rs // POST /graphql handler
├── subscriptions.rs   // WebSocket /graphql/subscriptions handler
├── introspection.rs   // Handle introspection queries
└── error_handler.rs   // Format GraphQL errors
```

**Responsibilities**:
- Parse HTTP POST body (JSON)
- Extract `query`, `variables`, `operationName`
- Call Rust GraphQL pipeline
- Handle errors
- Format response

### Layer 3: WebSocket Handler (Rust)

**Purpose**: Handle GraphQL subscriptions over WebSocket

```rust
// fraiseql_rs/src/http/websocket.rs
```

**Responsibilities**:
- Upgrade HTTP connection to WebSocket
- Handle GraphQL subscription protocol
- Send subscription updates
- Handle disconnections

*Note: Reuse existing subscription logic from Phase 15b*

### Layer 4: Python Bridge (Python)

**Purpose**: Provide user-facing API (unchanged)

```python
# src/fraiseql/http/
├── __init__.py        // create_fraiseql_app() factory
├── server.py          // RustHttpServer wrapper
├── config.py          // Configuration (port, host, etc.)
└── launcher.py        // Start Rust server in subprocess
```

**Responsibilities**:
- Provide `create_fraiseql_app()` function
- Load Rust HTTP server binary
- Configure and start server
- Log startup information

---

## 📋 Implementation Plan

### Phase 16 Structure: 4 Sub-phases

```
Phase 16a: HTTP Server Shell (2-3 days)
  - Basic Tokio server
  - Request routing
  - GraphQL handler (without subscriptions)

Phase 16b: Response Handling (1-2 days)
  - Response formatting
  - Error handling
  - JSON encoding (Rust)

Phase 16c: WebSocket & Subscriptions (2-3 days)
  - WebSocket upgrade
  - Subscription protocol
  - Connection management

Phase 16d: Testing & Polish (2-3 days)
  - Full test suite
  - Performance benchmarks
  - Documentation
```

### Phase 16a: HTTP Server Shell (Commits 1-3)

#### Commit 1: Basic HTTP Server Core

**File**: `fraiseql_rs/src/http/server.rs`

```rust
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use http::{StatusCode, HeaderMap};

/// Configuration for Rust HTTP server
pub struct HttpServerConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub request_timeout_ms: u64,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8000,
            max_connections: 10000,
            request_timeout_ms: 30000,
        }
    }
}

/// Main HTTP server structure
pub struct HttpServer {
    config: HttpServerConfig,
    listener: Option<TcpListener>,
}

impl HttpServer {
    pub fn new(config: HttpServerConfig) -> Self {
        Self {
            config,
            listener: None,
        }
    }

    /// Start the HTTP server
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;

        log::info!("FraiseQL HTTP server listening on {}", addr);
        self.listener = Some(listener);

        // Accept connections loop
        if let Some(listener) = &self.listener {
            loop {
                let (socket, peer_addr) = listener.accept().await?;
                log::debug!("New connection from {}", peer_addr);

                // Handle connection in background task
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(socket).await {
                        log::error!("Connection error: {}", e);
                    }
                });
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        self.listener = None;
        log::info!("HTTP server shutdown");
    }
}

/// Handle a single TCP connection
async fn handle_connection(mut socket: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    // Read HTTP request
    let mut buffer = vec![0; 8192]; // 8KB buffer
    let n = socket.read(&mut buffer).await?;

    if n == 0 {
        return Ok(()); // Connection closed
    }

    // Parse HTTP request (Commit 2)
    // Route request (Commit 2)
    // Handle GraphQL (Commit 3)

    Ok(())
}
```

**Testing**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_starts() {
        let mut config = HttpServerConfig::default();
        config.port = 9999; // Use random port
        let mut server = HttpServer::new(config);

        // Should not panic
        // Server will bind to port 9999
    }

    #[tokio::test]
    async fn test_server_shutdown() {
        let mut server = HttpServer::new(HttpServerConfig::default());
        server.shutdown().await;
        // Should clean shutdown without errors
    }
}
```

#### Commit 2: HTTP Request Parsing

**File**: `fraiseql_rs/src/http/request.rs`

```rust
use http::{Method, Uri, HeaderMap};
use serde::{Deserialize, Serialize};

/// Parsed GraphQL request from HTTP POST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLRequest {
    pub query: String,
    pub variables: Option<serde_json::Value>,
    pub operation_name: Option<String>,
}

/// Parse HTTP request line and headers
pub fn parse_http_request(buffer: &[u8]) -> Result<(Method, Uri, HeaderMap, usize), String> {
    // Find double CRLF that separates headers from body
    let headers_end = buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("Invalid HTTP request")?;

    let header_bytes = &buffer[..headers_end];
    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|_| "Invalid UTF-8 in headers")?;

    let mut lines = header_str.lines();

    // Parse request line: "POST /graphql HTTP/1.1"
    let request_line = lines.next().ok_or("Missing request line")?;
    let parts: Vec<&str> = request_line.split_whitespace().collect();

    if parts.len() != 3 {
        return Err("Invalid request line".to_string());
    }

    let method = Method::from_bytes(parts[0].as_bytes())
        .map_err(|_| "Invalid HTTP method")?;
    let uri = parts[1].parse::<Uri>()
        .map_err(|_| "Invalid URI")?;

    // Parse headers
    let mut headers = HeaderMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            let key = http::header::HeaderName::from_bytes(key.trim().as_bytes())
                .map_err(|_| "Invalid header name")?;
            let value = http::header::HeaderValue::from_str(value.trim())
                .map_err(|_| "Invalid header value")?;
            headers.insert(key, value);
        }
    }

    let body_start = headers_end + 4;
    Ok((method, uri, headers, body_start))
}

/// Parse GraphQL request from JSON body
pub fn parse_graphql_request(body: &[u8]) -> Result<GraphQLRequest, String> {
    serde_json::from_slice(body)
        .map_err(|e| format!("Invalid JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_request() {
        let request = b"POST /graphql HTTP/1.1\r\nHost: localhost:8000\r\nContent-Length: 50\r\n\r\n{\"query\":\"{ user { id } }\"}";

        let (method, uri, headers, body_start) = parse_http_request(request).unwrap();

        assert_eq!(method, Method::POST);
        assert_eq!(uri.path(), "/graphql");
        assert_eq!(body_start, request.len() - 24); // Points to JSON
    }

    #[test]
    fn test_parse_graphql_request() {
        let json = b"{\"query\":\"{ user { id } }\",\"variables\":null}";
        let req = parse_graphql_request(json).unwrap();

        assert_eq!(req.query, "{ user { id } }");
        assert_eq!(req.operation_name, None);
    }
}
```

#### Commit 3: Request Routing

**File**: `fraiseql_rs/src/http/routing.rs`

```rust
use http::{Method, Uri, StatusCode};

/// Route HTTP request to appropriate handler
pub enum Route {
    GraphQL,              // POST /graphql
    Subscriptions,        // WebSocket /graphql/subscriptions
    Introspection,        // GET /graphql (introspection query UI)
    HealthCheck,          // GET /health
    NotFound,
}

pub fn route_request(method: &Method, uri: &Uri) -> Route {
    match (method, uri.path()) {
        (Method::POST, "/graphql") => Route::GraphQL,
        (Method::GET, "/graphql") => Route::Introspection,
        (Method::GET, "/graphql/subscriptions") => Route::Subscriptions,
        (Method::GET, "/health") => Route::HealthCheck,
        _ => Route::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_graphql() {
        let method = Method::POST;
        let uri = "/graphql".parse().unwrap();
        assert!(matches!(route_request(&method, &uri), Route::GraphQL));
    }

    #[test]
    fn test_route_not_found() {
        let method = Method::GET;
        let uri = "/unknown".parse().unwrap();
        assert!(matches!(route_request(&method, &uri), Route::NotFound));
    }
}
```

**Cargo.toml updates**:
```toml
[dependencies]
# ... existing deps ...
http = "1.1"
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

### Phase 16b: Response Handling (Commits 4-6)

#### Commit 4: GraphQL Handler

**File**: `fraiseql_rs/src/http/graphql_handler.rs`

```rust
use crate::subscriptions::PyGraphQLRequest;
use http::{StatusCode, HeaderMap};
use serde_json::json;

pub struct GraphQLResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

/// Execute GraphQL query and return response
pub async fn handle_graphql_request(
    request: crate::request::GraphQLRequest,
    // Database pool, auth, etc. passed from Python
) -> Result<GraphQLResponse, String> {
    // Convert to PyGraphQLRequest (existing type)
    let py_request = PyGraphQLRequest {
        query: request.query,
        variables: request.variables.unwrap_or(json!({})),
        operation_name: request.operation_name,
    };

    // Call existing Rust pipeline (Phase 9)
    // This returns RustResponseBytes which is already JSON-encoded
    let response_bytes = execute_graphql_pipeline(py_request).await?;

    // Build HTTP response
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    headers.insert(
        http::header::CACHE_CONTROL,
        "no-store".parse().unwrap(),
    );

    Ok(GraphQLResponse {
        status: StatusCode::OK,
        headers,
        body: response_bytes.into_bytes().into_bytes(),
    })
}

/// Handle GraphQL errors
pub fn handle_graphql_error(error: String) -> GraphQLResponse {
    let body = json!({
        "errors": [{
            "message": error,
            "extensions": {
                "code": "INTERNAL_ERROR"
            }
        }]
    });

    GraphQLResponse {
        status: StatusCode::OK, // GraphQL spec: always 200 for parseable requests
        headers: {
            let mut h = HeaderMap::new();
            h.insert(
                http::header::CONTENT_TYPE,
                "application/json".parse().unwrap(),
            );
            h
        },
        body: body.to_string().into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_graphql_error_handling() {
        let response = handle_graphql_error("Query parsing failed".to_string());
        assert_eq!(response.status, StatusCode::OK);
        assert!(String::from_utf8(response.body)
            .unwrap()
            .contains("Query parsing failed"));
    }
}
```

#### Commit 5: Response Serialization

**File**: `fraiseql_rs/src/http/response.rs`

```rust
use http::{StatusCode, HeaderMap, Version};

/// Complete HTTP response
pub struct HttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Serialize to HTTP response bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut response = Vec::new();

        // Status line: "HTTP/1.1 200 OK"
        let status_text = self.status.canonical_reason().unwrap_or("Unknown");
        response.extend_from_slice(
            format!("HTTP/1.1 {} {}\r\n", self.status.as_u16(), status_text).as_bytes()
        );

        // Headers
        for (name, value) in &self.headers {
            response.extend_from_slice(name.as_str().as_bytes());
            response.extend_from_slice(b": ");
            response.extend_from_slice(value.as_bytes());
            response.extend_from_slice(b"\r\n");
        }

        // Content-Length header
        response.extend_from_slice(
            format!("Content-Length: {}\r\n", self.body.len()).as_bytes()
        );

        // Empty line
        response.extend_from_slice(b"\r\n");

        // Body
        response.extend_from_slice(&self.body);

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_serialization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );

        let response = HttpResponse {
            status: StatusCode::OK,
            headers,
            body: b"{\"data\": {}}".to_vec(),
        };

        let bytes = response.to_bytes();
        let s = String::from_utf8(bytes).unwrap();

        assert!(s.contains("HTTP/1.1 200 OK"));
        assert!(s.contains("application/json"));
        assert!(s.contains("{\"data\": {}}"));
    }
}
```

#### Commit 6: Error Handling

**File**: `fraiseql_rs/src/http/error_handler.rs`

```rust
use http::StatusCode;
use serde_json::json;

#[derive(Debug)]
pub enum HttpError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    InternalError(String),
}

impl HttpError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            HttpError::BadRequest(_) => StatusCode::BAD_REQUEST,
            HttpError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            HttpError::Forbidden(_) => StatusCode::FORBIDDEN,
            HttpError::NotFound(_) => StatusCode::NOT_FOUND,
            HttpError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_json(&self) -> Vec<u8> {
        let message = match self {
            HttpError::BadRequest(m) => m.clone(),
            HttpError::Unauthorized(m) => m.clone(),
            HttpError::Forbidden(m) => m.clone(),
            HttpError::NotFound(m) => m.clone(),
            HttpError::InternalError(m) => m.clone(),
        };

        let body = json!({
            "errors": [{
                "message": message,
                "extensions": {
                    "code": self.error_code()
                }
            }]
        });

        body.to_string().into_bytes()
    }

    fn error_code(&self) -> &str {
        match self {
            HttpError::BadRequest(_) => "BAD_REQUEST",
            HttpError::Unauthorized(_) => "UNAUTHORIZED",
            HttpError::Forbidden(_) => "FORBIDDEN",
            HttpError::NotFound(_) => "NOT_FOUND",
            HttpError::InternalError(_) => "INTERNAL_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_json() {
        let error = HttpError::BadRequest("Invalid query".to_string());
        let json_bytes = error.to_json();
        let json_str = String::from_utf8(json_bytes).unwrap();

        assert!(json_str.contains("Invalid query"));
        assert!(json_str.contains("BAD_REQUEST"));
    }
}
```

### Phase 16c: WebSocket & Subscriptions (Commits 7-9)

#### Commit 7: WebSocket Handler

**File**: `fraiseql_rs/src/http/websocket.rs`

```rust
use http::HeaderMap;
use tokio::net::TcpStream;

/// Handle WebSocket upgrade and GraphQL subscriptions
pub async fn handle_websocket_upgrade(
    stream: TcpStream,
    headers: &HeaderMap,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check for Upgrade header
    if headers
        .get(http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        != Some("websocket")
    {
        return Err("Not a WebSocket upgrade request".into());
    }

    // Get Sec-WebSocket-Key
    let ws_key = headers
        .get("sec-websocket-key")
        .ok_or("Missing Sec-WebSocket-Key")?
        .to_str()?;

    // Compute accept key (RFC 6455)
    let mut hasher = sha1::Sha1::new();
    hasher.update(ws_key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let digest = hasher.digest();
    let accept_key = base64::encode(digest.bytes());

    // Send WebSocket handshake response
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept_key
    );

    // (Connection would be upgraded from here)
    // Reuse existing subscription logic from Phase 15b

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_key_validation() {
        // Test WebSocket key handling
        let ws_key = "dGhlIHNhbXBsZSBub25jZQ==";
        let expected_accept = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

        // Verify hashing works correctly
        let mut hasher = sha1::Sha1::new();
        hasher.update(ws_key.as_bytes());
        hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
        let digest = hasher.digest();
        let accept_key = base64::encode(digest.bytes());

        assert_eq!(accept_key, expected_accept);
    }
}
```

Add to Cargo.toml:
```toml
sha1 = "0.10"
base64 = "0.22"
```

#### Commit 8: Connection Management

**File**: `fraiseql_rs/src/http/connection.rs`

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Track active connections for graceful shutdown
pub struct ConnectionManager {
    active_connections: Arc<AtomicUsize>,
    max_connections: usize,
}

impl ConnectionManager {
    pub fn new(max_connections: usize) -> Self {
        Self {
            active_connections: Arc::new(AtomicUsize::new(0)),
            max_connections,
        }
    }

    pub fn acquire(&self) -> Result<(), String> {
        let current = self.active_connections.load(Ordering::Relaxed);
        if current >= self.max_connections {
            return Err(format!(
                "Connection limit reached: {} active connections",
                current
            ));
        }
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn release(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn active_count(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_manager() {
        let manager = ConnectionManager::new(2);

        assert!(manager.acquire().is_ok());
        assert!(manager.acquire().is_ok());
        assert!(manager.acquire().is_err()); // Should hit limit

        manager.release();
        assert!(manager.acquire().is_ok());
    }
}
```

#### Commit 9: HTTP Module Integration

**File**: `fraiseql_rs/src/http/mod.rs`

```rust
pub mod connection;
pub mod error_handler;
pub mod graphql_handler;
pub mod request;
pub mod response;
pub mod routing;
pub mod server;
pub mod websocket;

pub use connection::ConnectionManager;
pub use error_handler::HttpError;
pub use graphql_handler::{handle_graphql_request, GraphQLResponse};
pub use request::{parse_graphql_request, parse_http_request, GraphQLRequest};
pub use response::HttpResponse;
pub use routing::{route_request, Route};
pub use server::{HttpServer, HttpServerConfig};
pub use websocket::handle_websocket_upgrade;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_module_exports() {
        // Verify all exports are available
        let _config = HttpServerConfig::default();
    }
}
```

### Phase 16d: Python Bridge & Testing (Commits 10-15)

#### Commit 10: Python HTTP Module

**File**: `src/fraiseql/http/__init__.py`

```python
"""Rust HTTP server integration for FraiseQL."""

from .server import RustHttpServer
from .config import RustHttpConfig
from .launcher import create_rust_http_app

__all__ = [
    "RustHttpServer",
    "RustHttpConfig",
    "create_rust_http_app",
]
```

#### Commit 11: Server Configuration

**File**: `src/fraiseql/http/config.py`

```python
"""Configuration for Rust HTTP server."""

from dataclasses import dataclass
from typing import Optional


@dataclass
class RustHttpConfig:
    """Configuration for Rust HTTP server.

    Attributes:
        host: Host to bind to (default: "0.0.0.0")
        port: Port to bind to (default: 8000)
        max_connections: Maximum concurrent connections (default: 10000)
        request_timeout_ms: Request timeout in milliseconds (default: 30000)
        workers: Number of worker threads (default: auto-detect CPU count)
        enable_compression: Enable gzip compression (default: True)
        enable_http2: Enable HTTP/2 support (default: True)
    """

    host: str = "0.0.0.0"
    port: int = 8000
    max_connections: int = 10000
    request_timeout_ms: int = 30000
    workers: Optional[int] = None
    enable_compression: bool = True
    enable_http2: bool = True

    def to_rust_dict(self) -> dict:
        """Convert to dict for Rust FFI."""
        return {
            "host": self.host,
            "port": self.port,
            "max_connections": self.max_connections,
            "request_timeout_ms": self.request_timeout_ms,
            "workers": self.workers or _get_cpu_count(),
            "enable_compression": self.enable_compression,
            "enable_http2": self.enable_http2,
        }


def _get_cpu_count() -> int:
    """Get CPU count for default worker configuration."""
    import os
    return os.cpu_count() or 4
```

#### Commit 12: Server Launcher

**File**: `src/fraiseql/http/server.py`

```python
"""Rust HTTP server implementation."""

import asyncio
import json
import logging
from typing import Any, Optional
from pathlib import Path

from fraiseql import _fraiseql_rs
from fraiseql.gql.schema_builder import build_fraiseql_schema
from graphql import GraphQLSchema

from .config import RustHttpConfig


logger = logging.getLogger(__name__)


class RustHttpServer:
    """Wrapper for Rust HTTP server."""

    def __init__(
        self,
        schema: GraphQLSchema,
        config: Optional[RustHttpConfig] = None,
        auth_provider: Any = None,
        db_pool: Any = None,
    ):
        """Initialize Rust HTTP server.

        Args:
            schema: GraphQL schema
            config: Server configuration
            auth_provider: Authentication provider
            db_pool: Database connection pool
        """
        self.schema = schema
        self.config = config or RustHttpConfig()
        self.auth_provider = auth_provider
        self.db_pool = db_pool
        self._server = None

    async def start(self) -> None:
        """Start the Rust HTTP server."""
        if _fraiseql_rs is None:
            raise RuntimeError(
                "Rust extension not available. "
                "Make sure fraiseql is installed correctly."
            )

        # Create Rust server instance
        rust_config = self.config.to_rust_dict()

        self._server = _fraiseql_rs.PyHttpServer(rust_config)

        # Start server
        await self._server.start()

        logger.info(
            f"FraiseQL Rust HTTP server started on "
            f"{self.config.host}:{self.config.port}"
        )

    async def shutdown(self) -> None:
        """Shutdown the server gracefully."""
        if self._server:
            await self._server.shutdown()
            logger.info("FraiseQL Rust HTTP server stopped")

    @property
    def is_running(self) -> bool:
        """Check if server is running."""
        return self._server is not None

    @property
    def active_connections(self) -> int:
        """Get count of active connections."""
        if self._server:
            return self._server.active_connections()
        return 0


def create_rust_http_app(
    schema: GraphQLSchema,
    config: Optional[RustHttpConfig] = None,
    auth_provider: Any = None,
    db_pool: Any = None,
) -> RustHttpServer:
    """Create and return Rust HTTP server.

    This is the drop-in replacement for create_fraiseql_app()
    for users who want to use the Rust HTTP server.

    Args:
        schema: GraphQL schema
        config: Server configuration
        auth_provider: Authentication provider
        db_pool: Database connection pool

    Returns:
        RustHttpServer instance ready to start

    Example:
        ```python
        from fraiseql.http import create_rust_http_app

        app = create_rust_http_app(schema=my_schema)
        await app.start()
        ```
    """
    return RustHttpServer(
        schema=schema,
        config=config,
        auth_provider=auth_provider,
        db_pool=db_pool,
    )
```

#### Commit 13: Python-Rust FFI Bindings

**File**: `fraiseql_rs/src/http/py_bindings.rs`

```rust
use pyo3::prelude::*;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Python wrapper for Rust HTTP server
#[pyclass]
pub struct PyHttpServer {
    runtime: Arc<Runtime>,
    server: Option<Box<crate::http::HttpServer>>,
}

#[pymethods]
impl PyHttpServer {
    #[new]
    fn new(config: std::collections::HashMap<String, PyObject>) -> PyResult<Self> {
        // Convert Python dict to Rust HttpServerConfig
        let host = config
            .get("host")
            .and_then(|v| v.extract::<String>().ok())
            .unwrap_or_else(|| "0.0.0.0".to_string());

        let port = config
            .get("port")
            .and_then(|v| v.extract::<u16>().ok())
            .unwrap_or(8000);

        let max_connections = config
            .get("max_connections")
            .and_then(|v| v.extract::<usize>().ok())
            .unwrap_or(10000);

        let request_timeout_ms = config
            .get("request_timeout_ms")
            .and_then(|v| v.extract::<u64>().ok())
            .unwrap_or(30000);

        let rust_config = crate::http::HttpServerConfig {
            host,
            port,
            max_connections,
            request_timeout_ms,
        };

        let runtime = Arc::new(
            Runtime::new().map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to create tokio runtime: {}", e),
            ))?
        );

        Ok(Self {
            runtime,
            server: Some(Box::new(crate::http::HttpServer::new(rust_config))),
        })
    }

    /// Start the HTTP server
    fn start(&mut self, py: Python) -> PyResult<&PyAny> {
        let runtime = Arc::clone(&self.runtime);

        pyo3_asyncio::tokio::future_into_py(py, async move {
            // Start server logic here
            Ok(())
        })
    }

    /// Shutdown the server
    fn shutdown(&mut self, py: Python) -> PyResult<&PyAny> {
        let runtime = Arc::clone(&self.runtime);

        pyo3_asyncio::tokio::future_into_py(py, async move {
            // Shutdown logic here
            Ok(())
        })
    }

    /// Get number of active connections
    fn active_connections(&self) -> usize {
        // Return from server
        0
    }
}
```

#### Commit 14: Comprehensive Tests

**File**: `tests/unit/http/test_http_server.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let config = HttpServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8000);
    }

    #[tokio::test]
    async fn test_request_parsing() {
        let request = b"POST /graphql HTTP/1.1\r\nHost: localhost\r\n\r\n{\"query\":\"query\"}";
        let (method, uri, _, body_start) = parse_http_request(request).unwrap();

        assert_eq!(method, Method::POST);
        assert_eq!(uri.path(), "/graphql");
    }

    #[test]
    fn test_routing() {
        let post_graphql = (Method::POST, "/graphql".parse().unwrap());
        assert!(matches!(route_request(&post_graphql.0, &post_graphql.1), Route::GraphQL));
    }

    #[test]
    fn test_error_response() {
        let error = HttpError::BadRequest("test".to_string());
        let json = error.to_json();
        assert!(!json.is_empty());
    }

    #[tokio::test]
    async fn test_connection_limits() {
        let manager = ConnectionManager::new(2);
        assert!(manager.acquire().is_ok());
        assert!(manager.acquire().is_ok());
        assert!(manager.acquire().is_err());
    }
}
```

**File**: `tests/integration/http/test_http_integration.py`

```python
"""Integration tests for Rust HTTP server."""

import pytest
import asyncio
import json
from fraiseql.http import create_rust_http_app, RustHttpConfig


@pytest.fixture
async def server(schema):
    """Create and start test server."""
    config = RustHttpConfig(port=9999)  # Use non-standard port
    server = create_rust_http_app(schema=schema, config=config)

    await server.start()
    yield server
    await server.shutdown()


@pytest.mark.asyncio
async def test_server_starts(server):
    """Test server starts successfully."""
    assert server.is_running


@pytest.mark.asyncio
async def test_graphql_request(server):
    """Test GraphQL request handling."""
    query = '{ user { id name } }'
    request_data = {"query": query}

    # Make request (would use httpx or similar)
    # response = await client.post("/graphql", json=request_data)
    # assert response.status_code == 200


@pytest.mark.asyncio
async def test_connection_tracking(server):
    """Test connection count tracking."""
    initial = server.active_connections
    # Make request...
    # assert server.active_connections >= initial


@pytest.mark.asyncio
async def test_graceful_shutdown(server):
    """Test graceful shutdown with active connections."""
    await server.shutdown()
    assert not server.is_running
```

#### Commit 15: Documentation

**File**: `docs/PHASE-16-HTTP-SERVER.md`

```markdown
# Phase 16: Native Rust HTTP Server

## Overview

Phase 16 replaces the Python HTTP layer (FastAPI/uvicorn) with a native Rust HTTP server while maintaining 100% backward compatibility.

## Why?

- **Performance**: 1.5-3x faster response times
- **Simplicity**: Single compiled binary, no Python HTTP layer
- **Consistency**: Pure Rust path from database to client
- **Reliability**: No GIL, better async handling

## Current Performance

- Python HTTP: 5-10ms overhead
- Rust pipeline: 7-12ms (Phases 1-15)
- **Total**: 12-22ms end-to-end

## After Phase 16

- Rust HTTP: <1ms overhead
- Rust pipeline: 7-12ms
- **Total**: 7-12ms end-to-end
- **Improvement**: 1.5-3x faster

## Architecture

### HTTP Server (Rust)
- Tokio-based async server
- HTTP/1.1 and HTTP/2 support
- WebSocket for subscriptions
- Connection pooling and limits

### Request Handler (Rust)
- JSON parsing
- Route matching
- Request validation
- Error handling

### Python Bridge (Python)
- `create_rust_http_app()` factory function
- Configuration management
- Logging integration

## Migration Guide

### Before (FastAPI)

```python
from fraiseql import create_fraiseql_app

app = create_fraiseql_app(schema=schema)

# Run with: uvicorn app:app --host 0.0.0.0 --port 8000
```

### After (Rust HTTP)

```python
from fraiseql.http import create_rust_http_app

app = create_rust_http_app(schema=schema)

# Run with: python -c "asyncio.run(app.start())"
```

## Configuration

```python
from fraiseql.http import RustHttpConfig, create_rust_http_app

config = RustHttpConfig(
    host="0.0.0.0",
    port=8000,
    max_connections=10000,
    enable_compression=True,
    enable_http2=True,
)

app = create_rust_http_app(schema=schema, config=config)
```

## Testing

### Unit Tests

```bash
cargo test -p fraiseql_rs http
```

### Integration Tests

```bash
pytest tests/integration/http/ -v
```

### Performance Benchmarks

```bash
pytest tests/performance/http/ -v
```

## Performance Targets

- Server startup: <100ms
- Request handling: <1ms
- Connection establish: <5ms
- Response serialization: <1ms

## Monitoring

The Rust HTTP server exposes metrics:
- Active connections
- Requests per second
- Average request latency
- Error rate
- Connection timeouts

## Troubleshooting

### Port Already in Use

```
Error: Address already in use
```

Solution: Change port in config or kill existing process:

```bash
lsof -i :8000
kill -9 <PID>
```

### High Memory Usage

```python
config = RustHttpConfig(
    max_connections=5000,  # Reduce from default 10000
)
```

### Request Timeouts

```python
config = RustHttpConfig(
    request_timeout_ms=60000,  # Increase from default 30000
)
```

## Backward Compatibility

- ✅ Identical Python API
- ✅ Same GraphQL responses
- ✅ WebSocket subscriptions work
- ✅ Authentication/RBAC unchanged
- ✅ Can switch back to FastAPI anytime

## Next Steps

See Phase 17+ plans for:
- HTTP/3 support
- Custom protocol handlers
- Advanced load balancing
- Distributed request tracking
```

---

## 📅 Implementation Timeline

### Week 1: HTTP Server Core
- **Day 1**: Basic server + request parsing
- **Day 2**: Routing + GraphQL handler
- **Day 3**: Response serialization + error handling

### Week 2: WebSocket & Testing
- **Day 1**: WebSocket upgrade + subscriptions
- **Day 2**: Connection management + monitoring
- **Day 3**: Full test suite

### Week 3: Python Bridge & Polish
- **Day 1**: Python FFI bindings
- **Day 2**: Configuration + documentation
- **Day 3**: Performance tuning + final tests

---

## 🧪 Testing Strategy

### Unit Tests (Rust)
```bash
# HTTP server tests
cargo test --lib http::

# All tests
cargo test --lib
```

**Expected coverage**: >95% of HTTP module

### Integration Tests (Python)
```bash
# HTTP server integration
pytest tests/integration/http/ -v

# Full integration suite
pytest tests/ -v
```

**Expected coverage**: All user-facing features

### Performance Tests
```bash
# Benchmark against FastAPI
pytest tests/performance/http_comparison.py -v
```

**Expected improvement**: 1.5-3x faster

### Chaos Tests
```bash
# Connection stress testing
pytest tests/chaos/http_stress.py -v
```

---

## 🎯 Success Criteria

### Functional
- ✅ Server starts/stops cleanly
- ✅ GraphQL requests work (identical responses to FastAPI)
- ✅ WebSocket subscriptions work
- ✅ Error handling matches FastAPI behavior
- ✅ All 5991+ existing tests pass

### Performance
- ✅ Response time: <5ms for cached queries (vs 7-12ms with FastAPI)
- ✅ Server startup: <100ms
- ✅ No regressions in Rust pipeline
- ✅ Memory usage: <50MB idle, <200MB under load

### Compatibility
- ✅ 100% backward compatible Python API
- ✅ No user code changes required
- ✅ Can switch back to FastAPI without changes

### Quality
- ✅ Zero clippy warnings
- ✅ Full test coverage (>95%)
- ✅ Documentation complete
- ✅ No regressions in existing tests

---

## 🚀 Rollout Strategy

### Phase 1: Development (Week 1-3)
- Implement on feature branch
- Local testing and iteration
- Code review and feedback

### Phase 2: Staging (Week 4)
- Deploy to staging environment
- Performance benchmarking
- Chaos testing
- Load testing with real queries

### Phase 3: Production (Week 5+)
- Feature flag for HTTP server selection
- Gradual rollout (1% → 10% → 50% → 100%)
- Monitor metrics (latency, errors, connections)
- Rollback plan: Switch back to FastAPI

### Feature Flag

```python
# In config
FRAISEQL_HTTP_SERVER = "rust"  # or "fastapi"
```

```python
# In app factory
if os.getenv("FRAISEQL_HTTP_SERVER") == "rust":
    from fraiseql.http import create_rust_http_app
    app = create_rust_http_app(schema)
else:
    from fraiseql import create_fraiseql_app
    app = create_fraiseql_app(schema)
```

---

## 📚 Dependencies

### Rust (Cargo.toml)
```toml
[dependencies]
tokio = { version = "1.35", features = ["full"] }
http = "1.1"
httpdate = "1.0"
base64 = "0.22"
sha1 = "0.10"
```

### Python (pyproject.toml)
No new dependencies! Uses existing `fraiseql._fraiseql_rs`.

---

## 🔄 Comparison: FastAPI vs Rust HTTP

| Aspect | FastAPI | Rust HTTP |
|--------|---------|-----------|
| **Startup time** | 100-200ms | <50ms |
| **Request latency** | 5-10ms | <1ms |
| **Memory (idle)** | 100-150MB | <50MB |
| **Connections/sec** | 1,000 | 5,000+ |
| **Code language** | Python | Rust |
| **Dependencies** | 50+ packages | 3 crates |
| **Binary size** | N/A | ~5MB |
| **GIL contention** | Yes | No |
| **Concurrency** | Limited | Excellent |

---

## 📝 Acceptance Criteria Checklist

### Code Quality
- [ ] All Rust code compiles without warnings
- [ ] All tests pass (unit + integration)
- [ ] Code coverage >95%
- [ ] No clippy warnings

### Functionality
- [ ] GraphQL queries work identically to FastAPI
- [ ] WebSocket subscriptions work
- [ ] Authentication/RBAC work
- [ ] Error responses match FastAPI format

### Performance
- [ ] Response time <5ms for cached queries
- [ ] Startup time <100ms
- [ ] No memory leaks
- [ ] Handles 10,000+ concurrent connections

### Documentation
- [ ] Migration guide written
- [ ] API documentation updated
- [ ] Examples provided
- [ ] Troubleshooting guide included

### Compatibility
- [ ] Python API unchanged
- [ ] No user code changes required
- [ ] Backward compatible
- [ ] Easy rollback to FastAPI

---

## 🎓 Learning Resources

### Tokio Documentation
- https://tokio.rs/
- https://tokio.rs/tokio/topics/io

### HTTP Specification
- https://www.rfc-editor.org/rfc/rfc7230
- https://www.rfc-editor.org/rfc/rfc7231

### WebSocket Protocol
- https://www.rfc-editor.org/rfc/rfc6455

### Rust Async Best Practices
- https://rust-lang.github.io/async-book/

---

## 🔗 Related Phases

**Previous**:
- Phase 15b: Tokio driver & subscriptions

**Next**:
- Phase 17: HTTP/2 & Protocol Optimizations
- Phase 18: Advanced Load Balancing
- Phase 19: Distributed Tracing Integration

---

## 📊 Metrics & Monitoring

### Key Metrics to Track

1. **Latency**
   - p50: <5ms
   - p95: <20ms
   - p99: <100ms

2. **Throughput**
   - Requests/sec
   - Connections/sec
   - Errors/sec

3. **Resource Usage**
   - Memory (MB)
   - CPU (%)
   - Connections (active)

4. **Errors**
   - 4xx responses
   - 5xx responses
   - Timeouts

### Dashboard Queries

```prometheus
# Latency
histogram_quantile(0.95, fraiseql_request_duration_ms)

# Throughput
rate(fraiseql_requests_total[1m])

# Connection count
fraiseql_active_connections

# Error rate
rate(fraiseql_errors_total[1m])
```

---

**Status**: ✅ Ready for Implementation

**Next Action**: Create feature branch and begin Phase 16a implementation

```bash
git checkout -b feature/phase-16-rust-http-server
```

