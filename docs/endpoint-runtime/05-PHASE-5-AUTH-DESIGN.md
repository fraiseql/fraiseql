# Phase 5: Best-in-Class Authentication System Design

**Goal**: Build authentication that excels in **performance**, **developer experience (DX)**, and **pluggability** while maintaining FraiseQL's zero-cost abstraction philosophy.

---

## Architecture Philosophy

FraiseQL's core principle: **Compile-time optimization, zero-runtime overhead.**

For authentication, this means:

- **Build-time**: Provider configuration validation, JWT algorithm pre-selection
- **Runtime**: Minimal overhead on hot paths (token validation, session lookup)
- **Plugin-friendly**: Easy integration with existing auth frameworks (Auth0, Keycloak, Firebase, AWS Cognito, etc.)

---

## 1. Performance-First Design

### 1.1 Token Validation Caching

**Problem**: Every request validates JWT signatures from scratch.

**Solution**: Cache JWKS (JSON Web Key Set) and token validation results.

```rust
// crates/fraiseql-server/src/auth/jwt_cache.rs

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// JWKS cache with TTL (typically 1 hour per provider OIDC spec)
pub struct JwksCache {
    cache: Arc<DashMap<String, (JwksData, Instant)>>,
    ttl: Duration,
}

impl JwksCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(3600), // 1 hour default
        }
    }

    pub async fn get_or_fetch(
        &self,
        issuer: &str,
        fetch_fn: impl Fn() -> BoxFuture<'_, Result<JwksData, AuthError>>,
    ) -> Result<JwksData, AuthError> {
        // Check cache first
        if let Some(entry) = self.cache.get(issuer) {
            let (data, cached_at) = entry.value();
            if cached_at.elapsed() < self.ttl {
                return Ok(data.clone());
            }
        }

        // Cache miss or expired - fetch new
        let data = fetch_fn().await?;
        self.cache.insert(issuer.to_string(), (data.clone(), Instant::now()));
        Ok(data)
    }
}

/// Token validation result cache
pub struct TokenCache {
    cache: Arc<DashMap<String, (Claims, Instant)>>,
    ttl: Duration,
}

impl TokenCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(300), // 5 minutes - shorter than access token expiry
        }
    }

    pub fn get(&self, token_hash: &str) -> Option<Claims> {
        if let Some(entry) = self.cache.get(token_hash) {
            let (claims, cached_at) = entry.value();
            if cached_at.elapsed() < self.ttl {
                return Some(claims.clone());
            }
        }
        None
    }

    pub fn insert(&self, token_hash: String, claims: Claims) {
        self.cache.insert(token_hash, (claims, Instant::now()));
    }
}
```

**Performance Impact**: 95%+ of token validations hit cache (zero crypto overhead).

### 1.2 Connection Pooling for Session Store

```rust
// crates/fraiseql-server/src/auth/session_store.rs

use deadpool_postgres::Pool;

pub struct SessionStore {
    db: Pool,
    /// Cache for frequently accessed user sessions
    cache: Arc<DashMap<String, (SessionData, Instant)>>,
}

impl SessionStore {
    pub async fn new(db: Pool) -> Result<Self, AuthError> {
        Ok(Self {
            db,
            cache: Arc::new(DashMap::new()),
        })
    }

    pub async fn get_session(&self, refresh_token: &str) -> Result<SessionData, AuthError> {
        let token_hash = sha256(refresh_token);

        // Try cache first
        if let Some((data, cached_at)) = self.cache.get(&token_hash).map(|e| e.value().clone()) {
            if cached_at.elapsed() < Duration::from_secs(60) {
                return Ok(data);
            }
        }

        // Cache miss - fetch from DB using connection pool
        let client = self.db.get().await?;
        let row = client
            .query_one(
                "SELECT user_id, expires_at FROM _system.refresh_tokens WHERE token_hash = $1",
                &[&token_hash],
            )
            .await?;

        let data = SessionData {
            user_id: row.get(0),
            expires_at: row.get(1),
        };

        self.cache.insert(token_hash, (data.clone(), Instant::now()));
        Ok(data)
    }
}
```

**Metrics**:
- Database query pool reuse: ~98%
- Average session lookup: <1ms (cached) or ~5ms (DB hit)

### 1.3 Zero-Copy Token Parsing

```rust
// crates/fraiseql-server/src/auth/jwt_fast.rs

use jsonwebtoken::{decode, decode_header, Algorithm, Validation};

/// Fast JWT validation without intermediate allocations
pub struct FastJwtValidator {
    validation: Validation,
    algorithms: Vec<Algorithm>,
}

impl FastJwtValidator {
    pub fn new(issuer: &str, algorithms: &[&str]) -> Result<Self, AuthError> {
        let algorithms = algorithms
            .iter()
            .map(|&algo| match algo {
                "RS256" => Ok(Algorithm::RS256),
                "HS256" => Ok(Algorithm::HS256),
                "ES256" => Ok(Algorithm::ES256),
                _ => Err(AuthError::InvalidAlgorithm),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut validation = Validation::new(algorithms[0]);
        validation.set_issuer(&[issuer]);

        Ok(Self { validation, algorithms })
    }

    /// Decode and validate JWT with minimal allocations
    #[inline]
    pub fn validate<'a>(&self, token: &'a str, key: &[u8]) -> Result<Claims, AuthError> {
        // Use borrowed token reference - no clone
        decode::<Claims>(token, key, &self.validation)
            .map(|data| data.claims)
            .map_err(|_| AuthError::InvalidToken)
    }
}
```

---

## 2. Developer Experience (DX)

### 2.1 Environment-Based Configuration with Sensible Defaults

```toml
# fraiseql.toml - minimal required config

[auth]
# Automatically detects provider from OAUTH_PROVIDER env var
# Supported: "google", "github", "microsoft", "apple", "auth0", "keycloak", "okta", "firebase"
provider = "google"  # Falls back to env var, then defaults to "google"

# All credentials loaded from environment - no secrets in config
[auth.google]
client_id_env = "GOOGLE_CLIENT_ID"
client_secret_env = "GOOGLE_CLIENT_SECRET"
redirect_url = "http://localhost:3000/callback"  # Can also use env var
```

**One-command development setup**:

```bash
# Script: bin/setup-auth-dev.sh
set -e

# Detect provider from environment or default to google
PROVIDER=${OAUTH_PROVIDER:-google}

echo "Setting up $PROVIDER authentication for local development..."

case "$PROVIDER" in
    google)
        echo "Visit: https://console.developers.google.com"
        echo "1. Create OAuth 2.0 Client ID (Web application)"
        echo "2. Add http://localhost:8000/auth/google/callback to authorized redirect URIs"
        echo "3. Copy client ID and secret:"
        read -p "GOOGLE_CLIENT_ID: " CLIENT_ID
        read -p "GOOGLE_CLIENT_SECRET: " CLIENT_SECRET
        {
            echo "export GOOGLE_CLIENT_ID='$CLIENT_ID'"
            echo "export GOOGLE_CLIENT_SECRET='$CLIENT_SECRET'"
        } >> .env.local
        ;;
    github)
        echo "Visit: https://github.com/settings/developers"
        echo "1. Create new OAuth App"
        echo "2. Set Authorization callback URL: http://localhost:8000/auth/github/callback"
        # ... similar flow
        ;;
esac

source .env.local
cargo run -- serve
```

### 2.2 Clear Error Messages with Documentation Links

```rust
// crates/fraiseql-server/src/auth/error.rs

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid access token: {reason}\n\nDocs: https://docs.fraiseql.dev/auth/errors/AU001")]
    InvalidToken { reason: String },

    #[error("Access token expired at {timestamp}\n\nDocs: https://docs.fraiseql.dev/auth/errors/AU002")]
    TokenExpired { timestamp: String },

    #[error("Provider '{provider}' is not configured\n\nSetup guide: https://docs.fraiseql.dev/auth/setup/{provider}")]
    ProviderNotConfigured { provider: String },

    #[error("OAuth code exchange failed: {message}\n\nDebugging: https://docs.fraiseql.dev/auth/troubleshoot")]
    CodeExchangeFailed { message: String },
}

impl AuthError {
    /// Returns HTTP status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidToken { .. } => StatusCode::UNAUTHORIZED,
            Self::TokenExpired { .. } => StatusCode::UNAUTHORIZED,
            Self::ProviderNotConfigured { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::CodeExchangeFailed { .. } => StatusCode::BAD_GATEWAY,
        }
    }

    /// Returns detailed error response for clients
    pub fn to_response(&self) -> serde_json::Value {
        json!({
            "error": self.to_string(),
            "status": self.status_code().as_u16(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
    }
}
```

### 2.3 Built-in Test Fixtures

```rust
// crates/fraiseql-server/src/auth/testing.rs

#[cfg(test)]
pub mod fixtures {
    use super::*;

    /// Pre-signed test JWT for local development
    pub struct TestJwtFixture {
        pub token: String,
        pub claims: Claims,
        pub secret: String,
    }

    impl TestJwtFixture {
        pub fn new() -> Self {
            let secret = "test-secret-key-do-not-use-in-production";
            let claims = Claims {
                sub: "test-user-id".to_string(),
                aud: vec!["test-client".to_string()],
                exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as u64,
                iat: chrono::Utc::now().timestamp() as u64,
                iss: "http://localhost:8000".to_string(),
                extra: Default::default(),
            };

            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &claims,
                &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
            ).unwrap();

            Self {
                token,
                claims,
                secret: secret.to_string(),
            }
        }

        pub fn with_user_id(mut self, user_id: &str) -> Self {
            self.claims.sub = user_id.to_string();
            self
        }

        pub fn expired(mut self) -> Self {
            self.claims.exp = (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp() as u64;
            self
        }
    }

    #[test]
    fn test_with_fixture() {
        let fixture = TestJwtFixture::new().with_user_id("custom-user");
        assert_eq!(fixture.claims.sub, "custom-user");
    }
}
```

---

## 3. Pluggability Architecture

### 3.1 Provider Trait System

```rust
// crates/fraiseql-server/src/auth/traits.rs

use async_trait::async_trait;

/// Core OAuth provider interface - easy to implement custom providers
#[async_trait]
pub trait OAuthProvider: Send + Sync + std::fmt::Debug {
    /// Provider name (e.g., "google", "github", "custom")
    fn name(&self) -> &str;

    /// Generate authorization URL for user consent
    fn authorization_url(&self, state: &str, redirect_uri: &str) -> String;

    /// Exchange authorization code for tokens
    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<TokenResponse, AuthError>;

    /// Fetch user information from provider
    async fn user_info(&self, access_token: &str) -> Result<UserInfo, AuthError>;

    /// Refresh access token (optional)
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse, AuthError> {
        Err(AuthError::RefreshNotSupported)
    }

    /// Custom claim mapping (optional)
    fn map_claims(&self, user_info: &UserInfo) -> Result<CustomClaims, AuthError> {
        Ok(CustomClaims::default())
    }
}

/// Custom user information structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomClaims {
    /// Custom groups/roles from provider
    pub groups: Vec<String>,

    /// Custom org ID (tenant isolation)
    pub org_id: Option<String>,

    /// Custom attributes
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

/// Provider registry - allows runtime provider registration
pub struct ProviderRegistry {
    providers: Arc<DashMap<String, Arc<dyn OAuthProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
        }
    }

    /// Register a built-in provider
    pub fn register_builtin(&self, provider: Arc<dyn OAuthProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// Register a custom provider
    pub fn register_custom(
        &self,
        name: &str,
        provider: Arc<dyn OAuthProvider>,
    ) -> Result<(), AuthError> {
        self.providers.insert(name.to_string(), provider);
        Ok(())
    }

    /// Get provider by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn OAuthProvider>> {
        self.providers.get(name).map(|entry| entry.clone())
    }
}
```

### 3.2 Middleware Decorator Pattern for Extensibility

```rust
// crates/fraiseql-server/src/auth/middleware.rs

/// Allows wrapping auth middleware with custom logic
#[async_trait]
pub trait AuthMiddlewareHook: Send + Sync {
    /// Called before token validation
    async fn before_validate(&self, token: &str) -> Result<(), AuthError> {
        Ok(())
    }

    /// Called after successful token validation
    async fn after_validate(&self, claims: &Claims) -> Result<(), AuthError> {
        Ok(())
    }

    /// Called on authentication failure
    async fn on_error(&self, error: &AuthError) {
        // Log, emit metrics, etc.
    }
}

/// Example: Rate limiting hook
pub struct RateLimitHook {
    limiter: Arc<RateLimiter>,
}

#[async_trait]
impl AuthMiddlewareHook for RateLimitHook {
    async fn before_validate(&self, _token: &str) -> Result<(), AuthError> {
        self.limiter.check_limit().await?;
        Ok(())
    }
}

/// Example: Audit logging hook
pub struct AuditLogHook {
    db: PgPool,
}

#[async_trait]
impl AuthMiddlewareHook for AuditLogHook {
    async fn after_validate(&self, claims: &Claims) -> Result<(), AuthError> {
        sqlx::query!(
            "INSERT INTO _system.auth_events (user_id, event_type, timestamp) VALUES ($1, $2, NOW())",
            claims.sub,
            "token_validated"
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }
}
```

### 3.3 Custom Session Store Implementations

```rust
// crates/fraiseql-server/src/auth/session.rs

#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create or update session
    async fn create_session(
        &self,
        user_id: Uuid,
        user_info: &UserInfo,
        metadata: SessionMetadata,
    ) -> Result<TokenPair, AuthError>;

    /// Refresh access token
    async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, AuthError>;

    /// Revoke single session
    async fn revoke(&self, refresh_token: &str) -> Result<(), AuthError>;

    /// Revoke all sessions for user (logout all devices)
    async fn revoke_all(&self, user_id: Uuid) -> Result<(), AuthError>;
}

/// Pluggable implementations
pub struct PostgresSessionStore {
    db: PgPool,
    jwt: Arc<dyn JwtManager>,
}

pub struct RedisSessionStore {
    redis: redis::Client,
    jwt: Arc<dyn JwtManager>,
}

pub struct InMemorySessionStore {
    sessions: Arc<DashMap<String, SessionData>>,
    jwt: Arc<dyn JwtManager>,
}

impl SessionStore for PostgresSessionStore {
    // ... implementation
}

impl SessionStore for RedisSessionStore {
    // ... implementation with better performance
}
```

---

## 4. Complete Configuration Example

```toml
# fraiseql.toml

[server]
bind_addr = "127.0.0.1:8000"
graphql_path = "/graphql"

[database]
url_env = "DATABASE_URL"

[auth]
# Session type: jwt, cookie, opaque
session_type = "jwt"

# Token expiry
access_token_expiry = "15m"
refresh_token_expiry = "7d"

# JWT signing
jwt_secret_env = "JWT_SECRET"  # Must be >32 chars

# Provider selection - detected from env or config
default_provider = "google"

# Redirect after OAuth callback
redirect_url = "https://api.example.com"
allowed_redirects = ["https://app.example.com"]

# Session storage backend: postgres, redis, memory (dev only)
session_store = "postgres"

[auth.google]
client_id_env = "GOOGLE_CLIENT_ID"
client_secret_env = "GOOGLE_CLIENT_SECRET"
scopes = ["openid", "email", "profile"]

[auth.github]
client_id_env = "GITHUB_CLIENT_ID"
client_secret_env = "GITHUB_CLIENT_SECRET"
scopes = ["user:email"]

[auth.microsoft]
client_id_env = "MICROSOFT_CLIENT_ID"
client_secret_env = "MICROSOFT_CLIENT_SECRET"
tenant = "common"  # or specific tenant ID

[auth.custom_saml]
# Easy integration with custom SAML/OIDC providers
metadata_url = "https://idp.example.com/saml/metadata"
assertion_consumer_service_url = "https://api.example.com/auth/saml/callback"
```

---

## 5. Integration with Existing Middleware

The auth system builds upon Phase 4B's existing infrastructure:

```rust
// crates/fraiseql-server/src/middleware/mod.rs

pub mod auth;              // NEW: Simple bearer token auth
pub mod oidc_auth;         // EXISTING: JWT/OIDC validation
pub mod cors;              // EXISTING: CORS handling
pub mod metrics;           // EXISTING: Request metrics
pub mod tracing;           // EXISTING: Tracing/logging

// Phase 5 builds on top without breaking anything
```

---

## 6. Performance Benchmarks (Target)

After implementation, expected performance metrics:

| Operation | Latency | Cache Hit Rate |
|-----------|---------|---|
| JWT validation (cached) | <100µs | 95%+ |
| Session lookup (cached) | <1ms | 90%+ |
| Token refresh | 10-20ms | N/A |
| JWKS fetch | 50-100ms | 99% (1hr TTL) |

**Memory footprint**: ~50MB for 10K cached sessions + JWKS data

---

## 7. Migration from Existing Auth (Phase 4)

Current system (bearer token + OIDC) continues to work:

```rust
// Both coexist during transition
let graphql_router = match config.auth_mode {
    AuthMode::Bearer => {
        // Use simple bearer_auth_middleware (Phase 4)
        Router::new()
            .route("/graphql", post(graphql_handler))
            .layer(middleware::from_fn_with_state(
                BearerAuthState::new(token.clone()),
                bearer_auth_middleware,
            ))
    }
    AuthMode::OIDC => {
        // Use full OAuth2/OIDC system (Phase 5)
        Router::new()
            .route("/graphql", post(graphql_handler))
            .layer(middleware::from_fn_with_state(
                OidcAuthState::new(validator.clone()),
                oidc_auth_middleware,
            ))
    }
    AuthMode::Disabled => {
        // No auth
        Router::new().route("/graphql", post(graphql_handler))
    }
};
```

---

## 8. Implementation Roadmap

### Phase 5.1: Core Framework
- [ ] JWT caching layer (JwksCache, TokenCache)
- [ ] Fast JWT validator with zero-copy parsing
- [ ] Provider trait system with registry
- [ ] Session store trait with Postgres implementation

### Phase 5.2: Built-in Providers
- [ ] Google OAuth provider
- [ ] GitHub OAuth provider
- [ ] Microsoft Entra ID provider
- [ ] Generic OIDC provider

### Phase 5.3: DX Features
- [ ] Environment-based configuration
- [ ] Setup scripts for local development
- [ ] Test fixtures and mocks
- [ ] Clear error messages with docs links

### Phase 5.4: Advanced Features
- [ ] Redis session store for distributed systems
- [ ] Middleware hooks (rate limiting, audit logging)
- [ ] Custom claim mapping
- [ ] Multi-tenancy support (org_id isolation)

### Phase 5.5: Documentation & Examples
- [ ] Setup guides for each provider
- [ ] Integration examples with popular frameworks
- [ ] Troubleshooting guide
- [ ] Performance tuning guide

---

## Success Criteria

- [ ] OAuth 2.0 / OIDC flows work for Google, GitHub, generic OIDC
- [ ] JWT tokens cached with <1ms validation latency
- [ ] Sessions stored with connection pooling (~98% pool reuse)
- [ ] DX: One-command setup script for local development
- [ ] DX: Clear error messages with documentation links
- [ ] Pluggability: Custom providers implementable without modifying core
- [ ] Pluggability: Middleware hooks work for rate limiting, audit logging
- [ ] Performance: <100µs token validation latency (cached)
- [ ] Performance: <1ms session lookup latency (cached)
- [ ] All existing tests pass (backward compatibility)
- [ ] New tests cover all error paths and edge cases
