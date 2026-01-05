# Axum Configuration Guide

**Version**: 2.0.0+
**Reading Time**: 25 minutes
**Audience**: Developers configuring Axum servers
**Prerequisites**: Completed [Axum Getting Started](./01-getting-started.md)

---

## Overview

This guide shows you how to configure your Axum HTTP server for different scenarios:
- ✅ CORS and cross-origin requests
- ✅ Authentication (JWT, Bearer tokens)
- ✅ Rate limiting and request throttling
- ✅ Custom middleware
- ✅ Advanced performance tuning
- ✅ Security hardening

---

## Basic Configuration Structure

Axum configuration in FraiseQL follows a builder pattern:

```rust
use fraiseql_rs::http::axum_server;

#[tokio::main]
async fn main() {
    let app = axum_server::create_router(pipeline)
        .with_cors(cors_config)
        .with_middleware(middleware_stack)
        .with_security(security_config);

    // Run server...
}
```

---

## CORS Configuration

CORS (Cross-Origin Resource Sharing) controls which domains can access your API.

### Minimal CORS Setup

**Allow single origin**:
```rust
use axum::middleware::CorsLayer;
use tower_http::cors::CorsLayer;

let cors = CorsLayer::permissive()  // Allow all origins (development only!)
    .allow_origin("http://localhost:3000".parse()?);

let app = Router::new()
    .layer(cors);
```

### Production CORS Configuration

**Restrict to specific origins**:
```rust
use tower_http::cors::{CorsLayer, AllowOrigin};
use http::HeaderValue;

let cors = CorsLayer::new()
    .allow_origin(
        "https://example.com"
            .parse::<HeaderValue>()?
            .parse::<AllowOrigin>()?
    )
    .allow_origin(
        "https://app.example.com"
            .parse::<HeaderValue>()?
            .parse::<AllowOrigin>()?
    )
    .allow_credentials(true)
    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
    .allow_headers([
        header::CONTENT_TYPE,
        header::AUTHORIZATION,
        "X-Requested-With".parse()?,
    ]);

let app = Router::new()
    .layer(cors);
```

### CORS with Environment Variables

**Load from config**:
```rust
use std::env;

fn build_cors() -> CorsLayer {
    let allowed_origins = env::var("CORS_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let mut cors = CorsLayer::new();

    for origin in allowed_origins.split(',') {
        cors = cors.allow_origin(
            origin.trim().parse::<HeaderValue>()?
                .parse::<AllowOrigin>()?
        );
    }

    cors.allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
        ])
}
```

**Run with environment variables**:
```bash
CORS_ORIGINS="https://example.com,https://app.example.com" cargo run
```

---

## Authentication Configuration

### JWT Token Validation

**Extract and validate JWT tokens**:
```rust
use fraiseql_rs::http::auth_middleware;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,  // Subject (user ID)
    exp: u64,     // Expiration time
    iat: u64,     // Issued at
}

async fn auth_layer(
    req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Extract token from Authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidFormat)?;

    // Validate token
    let secret = env::var("JWT_SECRET")
        .expect("JWT_SECRET not set");

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    ).map_err(|_| AuthError::InvalidToken)?;

    // Attach claims to request
    let mut req = req;
    req.extensions_mut().insert(token_data.claims);

    Ok(next.run(req).await)
}

#[derive(Debug)]
enum AuthError {
    MissingToken,
    InvalidFormat,
    InvalidToken,
}
```

### OAuth2 Integration

**Add OAuth2 for third-party authentication**:
```rust
use axum_oauth2::OAuth2;

let oauth2 = OAuth2::new()
    .provider("google")
    .client_id(env::var("GOOGLE_CLIENT_ID")?)
    .client_secret(env::var("GOOGLE_CLIENT_SECRET")?)
    .redirect_uri("http://localhost:8000/auth/callback");

let app = Router::new()
    .route("/auth/login", get(oauth2.login()))
    .route("/auth/callback", get(oauth2.callback()));
```

---

## Rate Limiting

### Token Bucket Rate Limiter

**Limit requests per IP**:
```rust
use tower_governor::{governor::RateLimiter, state::{DefaultState, NotKeyed}, state::NotKeyed};
use governor::Quota;
use std::num::NonZeroU32;

let rate_limiter = RateLimiter::direct(
    Quota::per_second(NonZeroU32::new(100).unwrap())  // 100 req/sec
);

async fn rate_limit_layer(
    req: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    let ip = req
        .headers()
        .get("X-Forwarded-For")
        .or(Some(&HeaderValue::from_static("127.0.0.1")))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1");

    if !rate_limiter.check_key(ip).is_ok() {
        return Err(RateLimitError::TooManyRequests);
    }

    Ok(next.run(req).await)
}
```

### Per-Operation Rate Limiting

**Limit GraphQL operations**:
```rust
// Limit mutations more strictly than queries
async fn graphql_handler(
    State(state): State<AppState>,
    body: Bytes,
) -> impl IntoResponse {
    let query: GraphQLQuery = serde_json::from_slice(&body)?;

    match query.operation_type {
        OperationType::Mutation => {
            // 10 mutations/second
            state.mutation_limiter.check_key("user")?;
        }
        OperationType::Query => {
            // 100 queries/second
            state.query_limiter.check_key("user")?;
        }
        _ => {}
    }

    // Execute query...
}
```

---

## Security Headers

### Add Security Headers Automatically

```rust
use tower_http::set_header::SetResponseHeaderLayer;
use http::header;

let security_headers = SetResponseHeaderLayer::if_not_present()
    .insert_if_not_present(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains".parse()?
    )
    .insert_if_not_present(
        header::X_FRAME_OPTIONS,
        "DENY".parse()?
    )
    .insert_if_not_present(
        header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse()?
    )
    .insert_if_not_present(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'".parse()?
    );

let app = Router::new()
    .layer(security_headers);
```

---

## Custom Middleware

### Create Custom Middleware

```rust
use axum::{
    middleware::{self, Next},
    http::Request,
    response::Response,
};
use tower::ServiceExt;

pub async fn logging_middleware<B>(
    req: Request<B>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let elapsed = start.elapsed();
    let status = response.status();

    println!(
        "{} {} - {} ({}ms)",
        method,
        uri,
        status,
        elapsed.as_millis()
    );

    response
}

pub async fn request_id_middleware<B>(
    mut req: Request<B>,
    next: Next,
) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    req.extensions_mut().insert(request_id);
    next.run(req).await
}

// Add middleware to router
let app = Router::new()
    .layer(middleware::from_fn(logging_middleware))
    .layer(middleware::from_fn(request_id_middleware));
```

---

## Response Compression

### Enable Compression

```rust
use tower_http::compression::CompressionLayer;

let compression = CompressionLayer::new()
    .br(true)   // Brotli
    .zstd(true) // Zstandard
    .gzip(true) // Gzip
    .compress_when(
        tower_http::compression::predicate::SizeAbove::new(1024)
    );

let app = Router::new()
    .layer(compression);
```

---

## Timeout Configuration

### Request Timeout

```rust
use tower::ServiceBuilder;
use tower::timeout::TimeoutLayer;
use std::time::Duration;

let timeout_layer = TimeoutLayer::new(Duration::from_secs(30));

let middleware = ServiceBuilder::new()
    .layer(timeout_layer)
    .into_inner();

let app = Router::new()
    .layer(middleware);
```

### Body Size Limits

```rust
use axum::extract::DefaultBodyLimit;

let body_limit = DefaultBodyLimit::max(1024 * 1024); // 1MB

let app = Router::new()
    .layer(body_limit);
```

---

## Production Configuration

### Complete Production Setup

```rust
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::post,
    Router,
};
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

#[tokio::main]
async fn main() {
    // Security headers
    let security_headers = SetResponseHeaderLayer::if_not_present()
        .insert_if_not_present(
            "strict-transport-security",
            "max-age=31536000; includeSubDomains".parse()?
        );

    // Compression
    let compression = CompressionLayer::new().br(true).gzip(true);

    // CORS
    let cors = build_cors_layer();

    // Middleware stack
    let middleware = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(compression)
        .layer(cors)
        .layer(security_headers)
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)); // 10MB

    // Router
    let app = Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/health", axum::routing::get(health_check))
        .layer(middleware);

    // Run server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000")
        .await?;

    println!("Server listening on 0.0.0.0:8000");

    axum::serve(listener, app).await?;
}
```

---

## Environment Variables

### Configuration via Environment

Create `.env` file:
```env
# Server
AXUM_HOST=0.0.0.0
AXUM_PORT=8000
AXUM_WORKERS=4

# Security
JWT_SECRET=your-secret-key-here
CORS_ORIGINS=https://example.com,https://app.example.com

# Database
DATABASE_URL=postgresql://user:pass@localhost/dbname
DATABASE_POOL_SIZE=20

# Performance
MAX_REQUEST_BODY_SIZE=10485760  # 10MB
REQUEST_TIMEOUT=30              # seconds

# Logging
RUST_LOG=info
LOG_REQUESTS=true
```

Load in your code:
```rust
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let host = env::var("AXUM_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("AXUM_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8000);

    let addr = format!("{}:{}", host, port);

    println!("Starting server on {}", addr);
}
```

---

## Common Configuration Scenarios

### Scenario 1: Public API

```rust
// Allow any origin (careful in production!)
let cors = CorsLayer::permissive();

// Aggressive rate limiting
let limiter = RateLimiter::direct(
    Quota::per_second(NonZeroU32::new(100).unwrap())
);

// Large body size for batched requests
let body_limit = DefaultBodyLimit::max(50 * 1024 * 1024);

let app = Router::new()
    .layer(cors)
    .layer(body_limit)
    .layer(rate_limit_middleware(limiter));
```

### Scenario 2: Internal API

```rust
// Restrict to internal IPs only
let cors = CorsLayer::new()
    .allow_origin("http://10.0.0.0".parse()?);

// Authentication required
let app = Router::new()
    .layer(middleware::from_fn(auth_middleware));

// Larger body size for internal tools
let body_limit = DefaultBodyLimit::max(100 * 1024 * 1024);
```

### Scenario 3: Mobile App Backend

```rust
// Mobile app origin
let cors = CorsLayer::new()
    .allow_origin("capacitor://localhost".parse()?);

// Token-based auth
let app = Router::new()
    .layer(middleware::from_fn(jwt_validation));

// Moderate rate limiting
let limiter = RateLimiter::direct(
    Quota::per_second(NonZeroU32::new(50).unwrap())
);
```

---

## Verification Checklist

After configuring your server:

- [ ] CORS allows expected origins
- [ ] Authentication is working
- [ ] Rate limiting is in place
- [ ] Security headers are present
- [ ] Request timeouts configured
- [ ] Body size limits appropriate
- [ ] Logging is enabled
- [ ] Compression is active

**Test your configuration**:
```bash
# Check CORS
curl -H "Origin: http://example.com" \
     -H "Access-Control-Request-Method: POST" \
     -X OPTIONS http://localhost:8000/graphql

# Check headers
curl -i http://localhost:8000/health

# Check rate limit
for i in {1..101}; do curl http://localhost:8000/graphql; done
```

---

## Next Steps

- **Ready to deploy?** → [Production Deployment](./03-deployment.md)
- **Need performance tuning?** → [Performance Tuning](./04-performance.md)
- **Something not working?** → [Troubleshooting](./05-troubleshooting.md)

---

## Quick Reference

| Configuration | Method | Example |
|---------------|--------|---------|
| CORS | `CorsLayer::new()` | Allow origins |
| Auth | Middleware | JWT validation |
| Rate limit | `RateLimiter` | 100 req/sec |
| Headers | `SetResponseHeaderLayer` | Security headers |
| Compression | `CompressionLayer` | Br + Gzip |
| Timeout | `TimeoutLayer` | 30 seconds |
| Body limit | `DefaultBodyLimit` | 10MB |

---

**Your server is now configured!** Time to deploy? → [Production Deployment](./03-deployment.md)
