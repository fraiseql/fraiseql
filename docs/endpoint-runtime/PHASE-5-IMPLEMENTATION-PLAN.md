# Phase 5: Authentication System - Implementation Plan

**Approved Design**: V2 (Stable Foundation) - No auth token caching
**Status**: Ready to implement
**Estimated Duration**: 2-3 weeks (Phase 5.1-5.4 core framework)

---

## Overview

Phase 5 builds a **simple, correct, extensible authentication system** for FraiseQL:

```
User Login Flow:
┌─ Client initiates: POST /auth/start
├─ FraiseQL redirects to OIDC provider
├─ User authenticates at provider
├─ OIDC provider redirects: GET /auth/callback?code=...
├─ FraiseQL exchanges code for tokens
├─ FraiseQL stores refresh token in SessionStore
├─ FraiseQL returns access token to client
└─ Client uses access token for API requests

Subsequent Requests:
┌─ Client sends: Authorization: Bearer <access_token>
├─ FraiseQL validates JWT (fresh, ~1-5ms)
├─ Attaches claims to request context
└─ Resolver can access authenticated user
```

---

## Phase Breakdown

### Phase 5.1: Core JWT Validation

**Goal**: Implement JWT validation that is simple, correct, and performant.

**Scope**:
- JWT parsing (header, payload, signature)
- Signature verification with public keys
- Expiry validation
- Standard claims extraction (sub, exp, iat, aud, iss)
- Custom claims handling
- Error types with actionable messages

**Deliverables**:

```rust
// crates/fraiseql-server/src/auth/jwt.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,              // Subject (user ID)
    pub iat: u64,                 // Issued at
    pub exp: u64,                 // Expiration
    pub iss: String,              // Issuer
    pub aud: Vec<String>,         // Audience
    pub extra: serde_json::Map,   // Custom claims
}

pub struct JwtValidator {
    validation: Validation,
}

impl JwtValidator {
    pub fn new(issuer: &str, algorithm: Algorithm) -> Result<Self>;
    pub fn validate(&self, token: &str, key: &[u8]) -> Result<Claims>;
}

pub enum AuthError {
    InvalidToken { reason: String },
    TokenExpired { timestamp: String },
    InvalidSignature,
    MissingClaim { claim: String },
}
```

**Tests**:
- Valid token with all claims
- Expired token rejected
- Invalid signature rejected
- Missing required claims rejected
- Custom claims extracted correctly
- Different algorithms (RS256, HS256, ES256)

**Files to Create**:
- `crates/fraiseql-server/src/auth/jwt.rs`
- `crates/fraiseql-server/src/auth/mod.rs`
- `crates/fraiseql-server/src/auth/error.rs`

**Duration**: 2-3 days

---

### Phase 5.2: Session Store & Management

**Goal**: Define a simple trait that developers implement for their storage backend.

**Scope**:
- SessionStore trait (4 methods, minimal interface)
- Session data structures
- PostgreSQL reference implementation
- In-memory test implementation
- Token generation and hashing
- Revocation (single session and all sessions)

**Deliverables**:

```rust
// crates/fraiseql-server/src/auth/session.rs

#[derive(Debug, Clone)]
pub struct SessionData {
    pub user_id: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub refresh_token_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair>;
    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData>;
    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()>;
    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()>;
}

// PostgreSQL reference implementation
pub struct PostgresSessionStore {
    db: PgPool,
    jwt_secret: Vec<u8>,
}

impl PostgresSessionStore {
    pub async fn new(db: PgPool, jwt_secret: &[u8]) -> Result<Self>;
}

#[async_trait]
impl SessionStore for PostgresSessionStore {
    // Implementation with proper schema, indexes, security
}

// In-memory test implementation
#[cfg(test)]
pub struct InMemorySessionStore {
    sessions: Arc<DashMap<String, SessionData>>,
}
```

**Tests**:
- Create session with valid data
- Get session by refresh token
- Revoke single session
- Revoke all sessions for user
- Expired session handling
- Token hashing security
- Concurrent access safety

**Database Schema**:
```sql
CREATE TABLE _system.sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_sessions_user_id ON _system.sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON _system.sessions(expires_at);
CREATE INDEX idx_sessions_revoked_at ON _system.sessions(revoked_at);
```

**Files to Create**:
- `crates/fraiseql-server/src/auth/session.rs`
- `crates/fraiseql-server/src/auth/session_postgres.rs`
- `crates/fraiseql-server/src/auth/testing.rs` (in-memory store)

**Duration**: 3-4 days

---

### Phase 5.3: OAuth Provider & OIDC

**Goal**: Implement a generic OIDC provider that works with any OIDC-compliant service.

**Scope**:
- OAuthProvider trait definition
- Generic OidcProvider implementation
- Authorization code flow with PKCE
- Token exchange
- User info retrieval
- Error handling for provider failures
- Support for multiple providers (Google, Keycloak, Auth0, custom)

**Deliverables**:

```rust
// crates/fraiseql-server/src/auth/provider.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub raw_claims: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}

#[async_trait]
pub trait OAuthProvider: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn authorization_url(&self, state: &str) -> String;
    async fn exchange_code(&self, code: &str) -> Result<TokenResponse>;
    async fn user_info(&self, access_token: &str) -> Result<UserInfo>;
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        Err(AuthError::OAuthError { message: "Not supported".to_string() })
    }
}

// Generic OIDC implementation
pub struct OidcProvider {
    name: String,
    issuer_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

impl OidcProvider {
    pub async fn new(
        name: &str,
        issuer_url: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
    ) -> Result<Self>;
}

#[async_trait]
impl OAuthProvider for OidcProvider {
    // Full OAuth 2.0 + OIDC flow implementation
}
```

**Features**:
- PKCE support (prevents authorization code interception)
- State parameter validation
- JWKS endpoint validation
- OIDC metadata discovery
- Proper error messages for common issues

**Tests**:
- Generate valid authorization URL with state
- Exchange code for tokens
- Get user info from provider
- Handle provider errors (invalid code, expired code, etc.)
- PKCE flow complete
- State validation prevents CSRF
- Multiple provider support

**Configuration Example**:
```toml
[auth.google]
issuer = "https://accounts.google.com"
client_id_env = "GOOGLE_CLIENT_ID"
client_secret_env = "GOOGLE_CLIENT_SECRET"
redirect_uri = "http://localhost:8000/auth/callback"
```

**Files to Create**:
- `crates/fraiseql-server/src/auth/provider.rs`
- `crates/fraiseql-server/src/auth/oidc_provider.rs`

**Duration**: 3-4 days

---

### Phase 5.4: Middleware & HTTP Endpoints

**Goal**: Integrate auth system into HTTP server with proper middleware and endpoints.

**Scope**:
- Authentication middleware (extract and validate token)
- OAuth flow endpoints (start, callback, refresh, logout)
- Error responses with helpful messages
- Integration with Axum router
- Request context attachment (claims available to resolvers)
- CORS handling for auth endpoints

**Deliverables**:

```rust
// crates/fraiseql-server/src/auth/middleware.rs

pub struct AuthMiddleware {
    validator: Arc<JwtValidator>,
    session_store: Arc<dyn SessionStore>,
}

impl AuthMiddleware {
    pub fn validate_token(&self, token: &str) -> Result<Claims>;
}

// Middleware layer for Axum
pub async fn auth_middleware(
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<impl IntoResponse>;

// HTTP endpoints
// POST /auth/start - Initiate OAuth flow
// GET /auth/callback - OAuth provider redirects here
// POST /auth/refresh - Refresh access token
// POST /auth/logout - Revoke session

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub claims: Claims,
}
```

**Endpoints**:

```
POST /auth/start
├─ Generates random state
├─ Stores state in session cache
├─ Redirects to OAuth provider
└─ Response: 302 redirect

GET /auth/callback?code=...&state=...
├─ Validates state (CSRF protection)
├─ Exchanges code for tokens
├─ Gets user info from provider
├─ Creates session in SessionStore
├─ Sets secure refresh token cookie
└─ Response: 302 redirect to app

POST /auth/refresh
├─ Validates refresh token
├─ Creates new access token
└─ Response: { access_token, expires_in }

POST /auth/logout
├─ Revokes session in SessionStore
├─ Clears refresh token cookie
└─ Response: 200 OK
```

**Error Handling**:
- Invalid token → 401 Unauthorized
- Expired token → 401 Unauthorized
- Missing Authorization header → 401 Unauthorized (if auth required)
- OAuth provider error → 502 Bad Gateway
- Session not found → 401 Unauthorized

**Tests**:
- Valid authorization header parsed
- Missing header handled correctly
- Expired token rejected
- Invalid signature rejected
- OAuth flow start generates state
- Callback validates state
- Session created on successful auth
- Refresh token endpoint works
- Logout revokes session
- Concurrent auth requests handled
- Multiple sessions per user

**Files to Create**:
- `crates/fraiseql-server/src/auth/middleware.rs`
- `crates/fraiseql-server/src/auth/handlers.rs`
- `crates/fraiseql-server/src/auth/mod.rs` (module exports)

**Duration**: 3-4 days

---

### Phase 5.5: Integration & Documentation

**Goal**: Integrate with query result caching, write docs and examples.

**Scope**:
- Query cache integration (invalidate on token revocation)
- Cache invalidation patterns
- Setup guides for common providers
- Configuration reference
- How-to guides (implement SessionStore, add custom provider)
- API documentation
- Example project (if time permits)

**Deliverables**:

**Documentation**:
1. **Setup Guide**: Step-by-step for Google OAuth
2. **Setup Guide**: Step-by-step for Keycloak
3. **Setup Guide**: Step-by-step for Auth0
4. **Setup Guide**: Generic OIDC provider
5. **Implementation Guide**: How to implement custom SessionStore
6. **Implementation Guide**: How to add custom OAuth provider
7. **API Reference**: All auth endpoints
8. **Configuration Reference**: All config options
9. **Troubleshooting**: Common issues and solutions
10. **Cache Invalidation**: How query cache works with auth

**Examples**:
```rust
// Example: Implement custom SessionStore for Redis
pub struct RedisSessionStore {
    client: redis::Client,
    jwt_secret: Vec<u8>,
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        // Implementation here
    }
    // ... other methods
}
```

**Files to Create**:
- `docs/auth/SETUP-GOOGLE.md`
- `docs/auth/SETUP-KEYCLOAK.md`
- `docs/auth/SETUP-AUTH0.md`
- `docs/auth/SETUP-OIDC-GENERIC.md`
- `docs/auth/IMPLEMENT-SESSION-STORE.md`
- `docs/auth/IMPLEMENT-OAUTH-PROVIDER.md`
- `docs/auth/API-REFERENCE.md`
- `docs/auth/CONFIGURATION.md`
- `docs/auth/TROUBLESHOOTING.md`
- `docs/auth/CACHE-INTEGRATION.md`

**Duration**: 3-4 days

---

### Phase 5.6: Production Ready & Monitoring

**Goal**: Add monitoring, documentation, and prepare for production deployment.

**Scope**:
- Performance metrics (auth latency)
- Structured logging
- Health checks
- Monitoring dashboard (Grafana, Prometheus)
- Security audit
- Production deployment guide
- Runbook for common issues

**Deliverables**:

**Metrics**:
```
fraiseql_auth_validation_duration_ms
├─ Histogram (p50, p95, p99)
├─ Tags: provider, status (success/failure)
└─ Alert: If p99 > 10ms

fraiseql_auth_cache_invalidation
├─ Counter
├─ Tags: reason (token_revocation, session_expired)
└─ Track cache coherency

fraiseql_oauth_flow_duration_ms
├─ Histogram (code exchange latency)
└─ Alert: If p99 > 200ms

fraiseql_session_store_latency_ms
├─ Histogram (create, get, revoke)
└─ Alert: If p99 > 50ms
```

**Logging**:
```
// Structured logs with context
{
  "event": "auth_token_validated",
  "user_id": "user123",
  "duration_ms": 2.5,
  "source": "auth_middleware",
  "timestamp": "2026-01-21T...",
  "request_id": "req-abc123"
}

{
  "event": "auth_token_invalid",
  "reason": "TokenExpired",
  "duration_ms": 0.5,
  "source": "auth_middleware"
}
```

**Files to Create**:
- `docs/auth/DEPLOYMENT.md`
- `docs/auth/MONITORING.md`
- `docs/auth/PERFORMANCE-TUNING.md`
- `docs/auth/SECURITY-CHECKLIST.md`
- `tools/monitoring/auth-dashboard.json` (Grafana)

**Duration**: 2-3 days

---

### Phase 5.7: Optional Optimization (Only if Needed)

**Trigger**: Production benchmarks show auth validation >50% of request time

**Scope** (if triggered):
- Token result caching with TTL
- JWKS caching with automatic refresh
- Cache invalidation strategy
- Performance benchmarking

**Not doing in Phase 5.1-5.6** (unless metrics prove necessary)

**Duration**: 1-2 weeks (only if triggered)

---

## Implementation Order

### Week 1: Core Framework
- **Day 1-2**: Phase 5.1 (JWT Validation)
- **Day 3-4**: Phase 5.2 (SessionStore)
- **Day 5**: Testing & integration

### Week 2: OAuth & Integration
- **Day 6-7**: Phase 5.3 (OIDC Provider)
- **Day 8-9**: Phase 5.4 (Middleware & Endpoints)
- **Day 10**: Testing & bug fixes

### Week 3: Documentation & Deployment
- **Day 11-12**: Phase 5.5 (Documentation)
- **Day 13-14**: Phase 5.6 (Monitoring & Deployment)
- **Day 15**: Final testing, polish

---

## Testing Strategy

### Unit Tests
- JWT validation (all error cases)
- Claims parsing
- SessionStore trait implementations
- OAuth provider flows
- Error handling

### Integration Tests
- Full OAuth flow (start → callback → token)
- Session creation and revocation
- Token refresh
- Multiple concurrent sessions
- Cache invalidation

### End-to-End Tests
- Login with Google OAuth
- Login with Keycloak
- Token refresh
- Session logout
- Permission changes (revocation)

### Security Tests
- CSRF protection (state validation)
- PKCE verification
- Token signature validation
- Expired token rejection
- Tampered token rejection

---

## Code Organization

```
crates/fraiseql-server/src/auth/
├── mod.rs                 # Module exports
├── error.rs               # Error types
├── jwt.rs                 # JWT validation
├── session.rs             # SessionStore trait
├── session_postgres.rs    # PostgreSQL impl
├── testing.rs             # In-memory impl for tests
├── provider.rs            # OAuthProvider trait
├── oidc_provider.rs       # Generic OIDC impl
├── middleware.rs          # Axum middleware
├── handlers.rs            # HTTP endpoint handlers
└── config.rs              # Configuration parsing
```

---

## Dependencies

### New Crate Dependencies
- `jsonwebtoken` - JWT validation
- `async-trait` - Async traits
- `serde_json` - JSON parsing
- `reqwest` - HTTP client for OAuth
- `uuid` - Session IDs
- `sha2` - Token hashing
- `axum` - Web framework
- `sqlx` - Database client
- `chrono` - Timestamp handling

### Existing Dependencies (from v1)
- `tokio` - Async runtime
- `thiserror` - Error types
- Already using most of these

---

## Definition of Done

Each phase complete when:
- [ ] Code written and reviewed
- [ ] Unit tests pass (100% coverage for auth code)
- [ ] Integration tests pass
- [ ] No clippy warnings
- [ ] Documentation written
- [ ] Example code works
- [ ] Commit message clear

---

## Success Metrics

**After Phase 5 Complete**:
- ✅ OAuth 2.0 / OIDC flows work
- ✅ JWT tokens validated correctly
- ✅ Sessions managed securely
- ✅ Token revocation effective (immediate)
- ✅ Multi-provider support (Google, Keycloak, Auth0, custom)
- ✅ Auth latency <5ms per request
- ✅ Simple developer experience
- ✅ Production ready
- ✅ Well documented

---

## Risk Mitigation

| Risk | Probability | Mitigation |
|------|-------------|-----------|
| OIDC metadata discovery fails | Low | Validate URL exists before init |
| Token validation too slow | Very Low | Monitor in Phase 5.6 |
| SessionStore becomes bottleneck | Low | Connection pooling, reference examples |
| Developers struggle with traits | Low | Clear docs, working examples |
| Security issue in implementation | Medium | Security review before release |

---

## Next Steps

1. ✅ Approve this plan
2. ⏳ Create GitHub issues for each phase
3. ⏳ Assign developers
4. ⏳ Schedule Phase 5.1 kickoff
5. ⏳ Setup development database

---

**Status**: Ready to implement
**Approved Design**: V2 (Stable Foundation)
**Estimated Timeline**: 2-3 weeks for core framework
