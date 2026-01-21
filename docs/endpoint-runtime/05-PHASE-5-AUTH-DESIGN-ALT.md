# Phase 5: Simple, Stable Authentication Foundation

**Goal**: Build authentication that prioritizes **stability, simplicity, and developer flexibility** while maintaining FraiseQL's zero-cost abstraction philosophy.

**Core Philosophy**: Define the protocol correctly. Let developers choose their own storage and optimization strategies. Optimize only when real performance data shows it's needed.

---

## Architecture Philosophy

FraiseQL's core principle: **Compile-time optimization, zero-runtime overhead.**

For authentication, this means:

- **Minimal abstraction layers**: Simple, auditable code that developers understand
- **Developer choice**: Don't prescribe storage solutions; let teams use what they already have
- **Clear protocols**: Well-defined interfaces so custom implementations are straightforward
- **Security-first**: Correct implementation matters more than micro-optimizations

---

## 1. Stable Foundation: JWT Validation

### 1.1 Simple, Correct JWT Validation

```rust
// crates/fraiseql-server/src/auth/jwt.rs

use jsonwebtoken::{decode, Algorithm, Validation};
use serde::{Deserialize, Serialize};

/// Standard JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (typically user ID)
    pub sub: String,

    /// Issued at timestamp
    pub iat: u64,

    /// Expiration timestamp
    pub exp: u64,

    /// Issuer
    pub iss: String,

    /// Audience
    pub aud: Vec<String>,

    /// Additional claims (for custom providers)
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// JWT validator - straightforward token validation
pub struct JwtValidator {
    validation: Validation,
}

impl JwtValidator {
    /// Create validator for specific issuer and algorithm
    pub fn new(issuer: &str, algorithm: Algorithm) -> Result<Self, AuthError> {
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[issuer]);
        validation.set_audience(&["fraiseql"]); // Configurable

        Ok(Self { validation })
    }

    /// Validate JWT signature and expiration
    ///
    /// This is the core validation - everything else is optional optimization.
    /// Returns claims if token is valid, error otherwise.
    pub fn validate(&self, token: &str, key: &[u8]) -> Result<Claims, AuthError> {
        let token_data = decode::<Claims>(token, key, &self.validation)
            .map_err(|e| AuthError::InvalidToken {
                reason: e.to_string(),
            })?;

        // Check expiration (redundant with jsonwebtoken but explicit)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if token_data.claims.exp < now {
            return Err(AuthError::TokenExpired {
                timestamp: token_data.claims.exp.to_string(),
            });
        }

        Ok(token_data.claims)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid token: {reason}")]
    InvalidToken { reason: String },

    #[error("Token expired at {timestamp}")]
    TokenExpired { timestamp: String },

    #[error("Provider '{provider}' not configured")]
    ProviderNotConfigured { provider: String },

    #[error("Session store error: {message}")]
    SessionStoreError { message: String },

    #[error("OAuth error: {message}")]
    OAuthError { message: String },
}

pub type Result<T> = std::result::Result<T, AuthError>;
```

**Why this approach**:
- Clear, auditable code
- No premature caching complexity
- Easy to understand what's happening
- Add caching later if benchmarks show it's needed

### 1.2 Configuration: Just the Essentials

```toml
# fraiseql.toml

[auth]
# Simple provider selection
provider = "oidc"

# Token configuration
access_token_expiry = "15m"
refresh_token_expiry = "7d"

# Secret key for signing (environment variable)
jwt_secret_env = "JWT_SECRET"

[auth.oidc]
# Generic OIDC provider - works with any OIDC-compliant provider
issuer = "https://auth.example.com"
client_id_env = "OIDC_CLIENT_ID"
client_secret_env = "OIDC_CLIENT_SECRET"
redirect_uri = "http://localhost:8000/auth/callback"
```

**Configuration Philosophy**:
- Only required settings in config
- All secrets from environment variables
- Let developers choose their provider
- Single generic OIDC provider handles 80% of use cases

---

## 2. Flexible Session Store: Developer Decides

### 2.1 Session Store Trait (Minimal Interface)

```rust
// crates/fraiseql-server/src/auth/session.rs

use async_trait::async_trait;

/// Session metadata (what the server needs to track)
#[derive(Debug, Clone)]
pub struct SessionData {
    pub user_id: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub refresh_token_hash: String, // Hashed for storage
}

/// Token pair returned after successful authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

/// Session store trait - implement for your storage system
///
/// This is the contract. You can implement it with:
/// - PostgreSQL (included example)
/// - Redis (included example)
/// - Memcached
/// - DynamoDB
/// - Any other system you already use
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create a new session after successful OAuth flow
    async fn create_session(
        &self,
        user_id: &str,
        expires_at: u64,
    ) -> Result<TokenPair>;

    /// Look up session by refresh token (hashed)
    ///
    /// This is called when:
    /// - Refreshing access tokens
    /// - Validating sessions
    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData>;

    /// Revoke a single session (logout from this device)
    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()>;

    /// Revoke all sessions for a user (logout from all devices)
    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()>;
}
```

**Why this approach**:
- Minimal surface area (4 methods)
- Clear contract developers understand
- No forced complexity
- Teams use what they already deployed

### 2.2 Reference Implementation: PostgreSQL

```rust
// crates/fraiseql-server/src/auth/session_postgres.rs

use crate::auth::session::{SessionData, SessionStore, TokenPair, AuthError, Result};
use sqlx::PgPool;
use uuid::Uuid;

/// Example PostgreSQL session store
///
/// Drop this into your project if you use PostgreSQL.
/// Otherwise, implement SessionStore for your storage system.
pub struct PostgresSessionStore {
    db: PgPool,
    jwt_secret: Vec<u8>,
}

impl PostgresSessionStore {
    pub async fn new(db: PgPool, jwt_secret: &[u8]) -> Result<Self> {
        // Create table on startup (idempotent)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS _system.sessions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id TEXT NOT NULL,
                refresh_token_hash TEXT NOT NULL UNIQUE,
                issued_at BIGINT NOT NULL,
                expires_at BIGINT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                revoked_at TIMESTAMPTZ
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON _system.sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON _system.sessions(expires_at);
            "#
        )
        .execute(&db)
        .await
        .map_err(|e| AuthError::SessionStoreError {
            message: e.to_string(),
        })?;

        Ok(Self {
            db,
            jwt_secret: jwt_secret.to_vec(),
        })
    }

    fn hash_token(token: &str) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(token);
        format!("{:x}", hasher.finalize())
    }
}

#[async_trait]
impl SessionStore for PostgresSessionStore {
    async fn create_session(
        &self,
        user_id: &str,
        expires_at: u64,
    ) -> Result<TokenPair> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate tokens (in real implementation, use jsonwebtoken)
        let refresh_token = Uuid::new_v4().to_string();
        let refresh_token_hash = Self::hash_token(&refresh_token);

        let access_token = create_jwt_token(user_id, now, expires_at, &self.jwt_secret)?;

        // Store session
        sqlx::query(
            "INSERT INTO _system.sessions (user_id, refresh_token_hash, issued_at, expires_at)
             VALUES ($1, $2, $3, $4)"
        )
        .bind(user_id)
        .bind(&refresh_token_hash)
        .bind(now as i64)
        .bind(expires_at as i64)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::SessionStoreError {
            message: e.to_string(),
        })?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: expires_at - now,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let row = sqlx::query_as::<_, (String, i64, i64, String)>(
            "SELECT user_id, issued_at, expires_at, refresh_token_hash
             FROM _system.sessions
             WHERE refresh_token_hash = $1 AND revoked_at IS NULL"
        )
        .bind(refresh_token_hash)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AuthError::SessionStoreError {
            message: e.to_string(),
        })?
        .ok_or(AuthError::SessionStoreError {
            message: "Session not found".to_string(),
        })?;

        Ok(SessionData {
            user_id: row.0,
            issued_at: row.1 as u64,
            expires_at: row.2 as u64,
            refresh_token_hash: row.3,
        })
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        sqlx::query(
            "UPDATE _system.sessions SET revoked_at = NOW() WHERE refresh_token_hash = $1"
        )
        .bind(refresh_token_hash)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::SessionStoreError {
            message: e.to_string(),
        })?;

        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE _system.sessions SET revoked_at = NOW() WHERE user_id = $1"
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AuthError::SessionStoreError {
            message: e.to_string(),
        })?;

        Ok(())
    }
}
```

**Why this example**:
- Shows how to implement the trait
- Production-ready PostgreSQL code
- Clear database schema
- Teams copy-paste or adapt to their needs

### 2.3 Another Example: Redis

```rust
// crates/fraiseql-server/src/auth/session_redis.rs

use crate::auth::session::{SessionData, SessionStore, TokenPair, AuthError, Result};
use redis::{Client, Commands};

/// Example Redis session store for distributed systems
///
/// Use this if you already have Redis deployed.
pub struct RedisSessionStore {
    client: Client,
    jwt_secret: Vec<u8>,
    ttl_seconds: usize,
}

impl RedisSessionStore {
    pub fn new(redis_url: &str, jwt_secret: &[u8], ttl_seconds: usize) -> Result<Self> {
        let client = Client::open(redis_url)
            .map_err(|e| AuthError::SessionStoreError {
                message: e.to_string(),
            })?;

        Ok(Self {
            client,
            jwt_secret: jwt_secret.to_vec(),
            ttl_seconds,
        })
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn create_session(
        &self,
        user_id: &str,
        expires_at: u64,
    ) -> Result<TokenPair> {
        // Implementation uses redis SET with EX
        // Store session data as JSON with TTL
        let mut conn = self.client.get_connection()
            .map_err(|e| AuthError::SessionStoreError {
                message: e.to_string(),
            })?;

        let refresh_token = Uuid::new_v4().to_string();
        let session_key = format!("session:{}", refresh_token);

        let session_data = serde_json::json!({
            "user_id": user_id,
            "expires_at": expires_at,
        });

        conn.set_ex(&session_key, session_data.to_string(), self.ttl_seconds)
            .map_err(|e| AuthError::SessionStoreError {
                message: e.to_string(),
            })?;

        // ... rest of implementation
        Ok(TokenPair {
            access_token: "...".to_string(),
            refresh_token,
            expires_in: self.ttl_seconds as u64,
        })
    }

    // ... other methods
}
```

**Developer Experience**:
- "I use Redis? Here's your implementation."
- "I use DynamoDB? Implement the trait (straightforward)."
- "I want a custom solution? Implement the trait."

---

## 3. OAuth Providers: Generic First

### 3.1 Provider Trait (Minimal)

```rust
// crates/fraiseql-server/src/auth/provider.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// User information returned by OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,

    /// Raw claims from provider (for custom handling)
    pub raw_claims: serde_json::Value,
}

/// OAuth provider interface
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// Provider name for logging/identification
    fn name(&self) -> &str;

    /// Build authorization URL for user to visit
    fn authorization_url(&self, state: &str) -> String;

    /// Exchange authorization code for tokens
    async fn exchange_code(&self, code: &str) -> Result<TokenResponse>;

    /// Get user information using access token
    async fn user_info(&self, access_token: &str) -> Result<UserInfo>;

    /// Optional: Refresh token support
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        Err(AuthError::OAuthError {
            message: "Provider does not support refresh tokens".to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}
```

### 3.2 Generic OIDC Provider (One Implementation)

```rust
// crates/fraiseql-server/src/auth/oidc_provider.rs

use crate::auth::provider::{OAuthProvider, TokenResponse, UserInfo, AuthError, Result};
use reqwest::Client;

/// Generic OIDC provider - works with any OIDC-compliant service
///
/// This single implementation handles:
/// - Auth0, Keycloak, Okta, Azure AD, custom OIDC providers
/// - Just provide issuer URL and credentials
pub struct OidcProvider {
    name: String,
    issuer_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    http_client: Client,
}

impl OidcProvider {
    pub async fn new(
        name: &str,
        issuer_url: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
    ) -> Result<Self> {
        let provider = Self {
            name: name.to_string(),
            issuer_url: issuer_url.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            http_client: Client::new(),
        };

        // Validate provider is reachable and has OIDC metadata
        let metadata_url = format!("{}/.well-known/openid-configuration", issuer_url);
        provider.http_client.get(&metadata_url)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: format!("Failed to load OIDC metadata: {}", e),
            })?;

        Ok(provider)
    }
}

#[async_trait]
impl OAuthProvider for OidcProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn authorization_url(&self, state: &str) -> String {
        format!(
            "{}/authorize?client_id={}&redirect_uri={}&response_type=code&state={}&scope=openid+email+profile",
            self.issuer_url,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_uri),
            urlencoding::encode(state),
        )
    }

    async fn exchange_code(&self, code: &str) -> Result<TokenResponse> {
        let response = self.http_client
            .post(format!("{}/token", self.issuer_url))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("redirect_uri", &self.redirect_uri),
            ])
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: e.to_string(),
            })?;

        let token_data: serde_json::Value = response.json().await
            .map_err(|e| AuthError::OAuthError {
                message: e.to_string(),
            })?;

        Ok(TokenResponse {
            access_token: token_data["access_token"].as_str().unwrap().to_string(),
            refresh_token: token_data["refresh_token"].as_str().map(|s| s.to_string()),
            expires_in: token_data["expires_in"].as_u64().unwrap_or(3600),
            token_type: "Bearer".to_string(),
        })
    }

    async fn user_info(&self, access_token: &str) -> Result<UserInfo> {
        let response = self.http_client
            .get(format!("{}/userinfo", self.issuer_url))
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError {
                message: e.to_string(),
            })?;

        let raw_claims: serde_json::Value = response.json().await
            .map_err(|e| AuthError::OAuthError {
                message: e.to_string(),
            })?;

        Ok(UserInfo {
            id: raw_claims["sub"].as_str().unwrap().to_string(),
            email: raw_claims["email"].as_str().unwrap_or("").to_string(),
            name: raw_claims["name"].as_str().map(|s| s.to_string()),
            picture: raw_claims["picture"].as_str().map(|s| s.to_string()),
            raw_claims,
        })
    }
}
```

**Why this approach**:
- One implementation handles 90% of use cases
- Easy to add more providers if needed
- No provider registry complexity initially
- Teams can implement custom providers by copying this pattern

---

## 4. Middleware Integration (Straightforward)

```rust
// crates/fraiseql-server/src/auth/middleware.rs

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::IntoResponse,
};
use tower::Layer;

/// Extract JWT from Authorization header
fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Authentication middleware layer
pub async fn auth_middleware(
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<impl IntoResponse, AuthError> {
    let token = extract_token(&headers)
        .ok_or(AuthError::InvalidToken {
            reason: "Missing Authorization header".to_string(),
        })?;

    // Validate token
    let claims = JWT_VALIDATOR.validate(&token, JWT_SECRET)?;

    // Attach claims to request for handlers to use
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}
```

---

## 5. Simple Configuration

```toml
# fraiseql.toml

[auth]
# Choose your provider
provider = "oidc"

# Token settings
access_token_expiry = "15m"
refresh_token_expiry = "7d"

# Secret (from env)
jwt_secret_env = "JWT_SECRET"

# Session store (implement the trait)
session_store = "postgres"  # or "redis", or your custom impl

[auth.oidc]
# Any OIDC provider works here
issuer = "https://accounts.google.com"
client_id_env = "OIDC_CLIENT_ID"
client_secret_env = "OIDC_CLIENT_SECRET"
redirect_uri = "http://localhost:8000/auth/callback"
```

---

## 6. Testing Support

```rust
// crates/fraiseql-server/src/auth/testing.rs

#[cfg(test)]
pub mod fixtures {
    use super::*;

    /// In-memory session store for testing (no database needed)
    pub struct InMemorySessionStore {
        sessions: Arc<DashMap<String, SessionData>>,
        jwt_secret: Vec<u8>,
    }

    impl InMemorySessionStore {
        pub fn new() -> Self {
            Self {
                sessions: Arc::new(DashMap::new()),
                jwt_secret: b"test-secret".to_vec(),
            }
        }
    }

    #[async_trait]
    impl SessionStore for InMemorySessionStore {
        async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
            let refresh_token = Uuid::new_v4().to_string();
            self.sessions.insert(
                refresh_token.clone(),
                SessionData {
                    user_id: user_id.to_string(),
                    issued_at: 0,
                    expires_at,
                    refresh_token_hash: refresh_token.clone(),
                },
            );
            Ok(TokenPair {
                access_token: "test-token".to_string(),
                refresh_token,
                expires_in: 3600,
            })
        }
        // ... other methods
    }

    #[tokio::test]
    async fn test_session_creation() {
        let store = InMemorySessionStore::new();
        let pair = store.create_session("user123", 9999999999).await.unwrap();
        assert!(!pair.refresh_token.is_empty());
    }
}
```

---

## 7. Implementation Roadmap

### Phase 5.1: Core JWT Validation
- [ ] JWT validator (sign, verify, expiration)
- [ ] Claims structure
- [ ] Error handling
- [ ] Unit tests

### Phase 5.2: Session Store Trait
- [ ] SessionStore trait definition
- [ ] PostgreSQL reference implementation
- [ ] In-memory implementation for testing
- [ ] Integration tests

### Phase 5.3: OIDC Provider
- [ ] Generic OIDC provider
- [ ] Authorization URL generation
- [ ] Code exchange
- [ ] User info retrieval

### Phase 5.4: Middleware & Endpoints
- [ ] Authentication middleware
- [ ] POST /auth/start (initiate OAuth)
- [ ] GET /auth/callback (OAuth callback)
- [ ] POST /auth/refresh (refresh token)
- [ ] POST /auth/logout (revoke session)

### Phase 5.5: Documentation & Examples
- [ ] Setup guide for OIDC provider
- [ ] How to implement SessionStore for your storage
- [ ] How to add custom OAuth provider
- [ ] Testing guide
- [ ] Configuration reference

---

## 8. Migration from Phase 4

Existing bearer token auth continues to work:

```rust
// Both can coexist
match auth_config.mode {
    AuthMode::Bearer => {
        // Phase 4: Simple bearer token
        router.layer(BearerAuthLayer::new())
    }
    AuthMode::Oauth => {
        // Phase 5: Full OAuth/OIDC
        router.layer(OAuthAuthLayer::new(provider, session_store))
    }
}
```

---

## 9. Why This Approach

| Aspect | This Design | Alternative (V1) |
|--------|------------|------------------|
| **Learning curve** | Start simple | Complex caching upfront |
| **Flexibility** | Choose your storage | Prescribed solutions |
| **Code clarity** | Easy to audit | More layers to understand |
| **Optimization path** | Add caching when needed | Already complex |
| **Debugging** | Straightforward | Cache invalidation issues |
| **Long-term maintenance** | Simpler to evolve | More to maintain |
| **Developer choice** | Use what you have | Learn new systems |

---

## 10. When to Optimize

**Only add complexity when benchmarks show it's needed**:

- If JWT validation latency > 5ms on average → Add token validation cache
- If session store queries > 50ms → Add session cache or optimize queries
- If many parallel OAuth flows → Add rate limiting middleware
- If JWKS changes frequently → Add JWKS cache

Each optimization is isolated (the trait system allows it):
- Add JwtCache without changing SessionStore
- Add RedisSessionStore without touching OIDC provider
- Add middleware hooks without breaking existing code

---

## 11. Success Criteria

- [ ] JWT validation works (sign, verify, expiration)
- [ ] SessionStore trait is implementable by developers
- [ ] PostgreSQL reference implementation is production-ready
- [ ] Generic OIDC provider works with major providers
- [ ] Middleware integrates cleanly with existing framework
- [ ] Tests pass (JWT, session store, provider)
- [ ] Documentation shows how to implement custom SessionStore
- [ ] Configuration is minimal and environment-based
- [ ] Backward compatible with Phase 4 bearer token auth
- [ ] No premature complexity

---

## 12. Comparison: Core Philosophies

**Stable Foundation Approach** (This Document):
- Start with correct, auditable core
- Let developers control storage
- Add caching only when needed
- Zero forced complexity
- Easier to maintain long-term

**Performance-First Approach** (V1 Document):
- Optimize for speed immediately
- Provide multiple storage options
- Built-in caching layers
- More features out of the box
- More code to maintain

**Decision**: Which philosophy better serves FraiseQL's goals?

Both are valid. This document presents the alternative for comparison.
