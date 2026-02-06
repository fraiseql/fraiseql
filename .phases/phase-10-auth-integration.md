# Phase 10: Authentication & Token Validation in Rust

**Objective**: Move JWT token validation, user context extraction, and authentication logic from Python to Rust for 5-10x performance improvement and reduced Python overhead.

**Current State**: Authentication happens in Python (Auth0Provider, JWT validation) before GraphQL execution

**Target State**: Rust handles all token validation, user extraction, and auth errors with zero Python overhead

---

## Context

**Why This Phase Matters:**

- Token validation is on the critical path (every request)
- JWT libraries in Rust (jsonwebtoken) are 5-10x faster than Python PyJWT
- Eliminates Python auth provider overhead
- Enables auth caching in Rust for sub-millisecond validation

**Dependencies:**

- Phase 9 (Unified Pipeline) ✅ Complete
- Rust GraphQL execution pipeline
- UserContext struct already exists in unified.rs

**Performance Target:**

- JWT validation: <1ms (currently ~5-10ms in Python)
- Cached user context: <0.1ms
- Auth0 JWKS fetch: <50ms (cached for 1 hour)

---

## Files to Modify/Create

### Rust Files (fraiseql_rs/src/auth/)

- **mod.rs** (NEW): Auth module exports
- **jwt.rs** (NEW): JWT token validation with jsonwebtoken crate
- **provider.rs** (NEW): Auth provider trait (Auth0, JWT, custom)
- **cache.rs** (NEW): User context caching with LRU
- **errors.rs** (NEW): Auth error types (TokenExpired, InvalidToken, etc.)

### Integration Files

- **fraiseql_rs/src/lib.rs**: Add auth module, PyAuth class
- **fraiseql_rs/src/pipeline/unified.rs**: Integrate auth validation before GraphQL execution
- **fraiseql_rs/Cargo.toml**: Add dependencies (jsonwebtoken, reqwest for JWKS)

### Python Migration Files

- **src/fraiseql/auth/rust_provider.py** (NEW): Python wrapper for Rust auth
- **src/fraiseql/auth/base.py**: Keep interface, deprecate Python implementations

### Test Files

- **tests/test_rust_auth.py** (NEW): Integration tests for Rust auth
- **tests/unit/auth/test_jwt_validation.rs** (NEW): Rust unit tests

---

## Implementation Steps

### Step 1: Rust JWT Validation Core (jwt.rs)

```rust
//! JWT token validation with Auth0/custom JWKS support.

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use std::collections::HashMap;

/// JWT claims structure (Auth0 compatible)
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // User ID
    pub email: Option<String>,
    pub name: Option<String>,
    pub exp: usize,
    pub iat: usize,
    pub iss: String,  // Issuer
    pub aud: Vec<String>,  // Audience

    // Auth0 custom claims
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// JWT validator with JWKS support
pub struct JWTValidator {
    issuer: String,
    audience: Vec<String>,
    jwks_url: String,
    jwks_cache: JWKSCache,
    algorithms: Vec<Algorithm>,
}

impl JWTValidator {
    pub fn new(issuer: String, audience: Vec<String>, jwks_url: String) -> Self {
        Self {
            issuer,
            audience,
            jwks_url,
            jwks_cache: JWKSCache::new(),
            algorithms: vec![Algorithm::RS256],
        }
    }

    /// Validate JWT token and return claims
    pub async fn validate(&self, token: &str) -> Result<Claims> {
        // 1. Decode header to get key ID (kid)
        let header = decode_header(token)?;
        let kid = header.kid.ok_or_else(|| anyhow!("Missing kid in token header"))?;

        // 2. Get public key from JWKS (cached)
        let public_key = self.jwks_cache.get_key(&kid, &self.jwks_url).await?;

        // 3. Validate token
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&self.audience);

        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_rsa_pem(public_key.as_bytes())?,
            &validation,
        )?;

        Ok(token_data.claims)
    }
}

/// JWKS cache with 1-hour TTL
struct JWKSCache {
    cache: Arc<Mutex<HashMap<String, (String, SystemTime)>>>,
}

impl JWKSCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get public key by kid (fetches from JWKS if not cached)
    pub async fn get_key(&self, kid: &str, jwks_url: &str) -> Result<String> {
        // Check cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some((key, cached_at)) = cache.get(kid) {
                // Check if cache is still valid (1 hour TTL)
                let elapsed = SystemTime::now().duration_since(*cached_at)?;
                if elapsed.as_secs() < 3600 {
                    return Ok(key.clone());
                }
            }
        }

        // Fetch JWKS from URL
        let jwks = self.fetch_jwks(jwks_url).await?;

        // Find key by kid
        let key = jwks.keys.iter()
            .find(|k| k.kid == kid)
            .ok_or_else(|| anyhow!("Key not found: {}", kid))?;

        // Convert JWK to PEM
        let public_key = jwk_to_pem(key)?;

        // Cache the key
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(kid.to_string(), (public_key.clone(), SystemTime::now()));
        }

        Ok(public_key)
    }

    async fn fetch_jwks(&self, url: &str) -> Result<JWKS> {
        let response = reqwest::get(url).await?;
        let jwks: JWKS = response.json().await?;
        Ok(jwks)
    }
}

#[derive(Deserialize)]
struct JWKS {
    keys: Vec<JWK>,
}

#[derive(Deserialize)]
struct JWK {
    kid: String,
    kty: String,
    n: String,
    e: String,
}

fn jwk_to_pem(jwk: &JWK) -> Result<String> {
    // Convert JWK (n, e) to PEM format
    // Implementation uses base64 decoding + ASN.1 encoding
    // (Simplified for phase plan - full implementation needed)
    todo!("Implement JWK to PEM conversion")
}
```

### Step 2: Auth Provider Trait (provider.rs)

```rust
//! Authentication provider trait and implementations.

use async_trait::async_trait;
use anyhow::Result;
use crate::pipeline::unified::UserContext;

/// Auth provider trait (supports Auth0, JWT, custom)
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Validate token and extract user context
    async fn validate_token(&self, token: &str) -> Result<UserContext>;

    /// Optional: Refresh token
    async fn refresh_token(&self, refresh_token: &str) -> Result<(String, String)> {
        Err(anyhow::anyhow!("Token refresh not supported"))
    }

    /// Optional: Revoke token
    async fn revoke_token(&self, token: &str) -> Result<()> {
        Err(anyhow::anyhow!("Token revocation not supported"))
    }
}

/// Auth0 provider implementation
pub struct Auth0Provider {
    validator: JWTValidator,
}

impl Auth0Provider {
    pub fn new(domain: &str, audience: Vec<String>) -> Self {
        let issuer = format!("https://{}/", domain);
        let jwks_url = format!("https://{}/.well-known/jwks.json", domain);

        Self {
            validator: JWTValidator::new(issuer, audience, jwks_url),
        }
    }
}

#[async_trait]
impl AuthProvider for Auth0Provider {
    async fn validate_token(&self, token: &str) -> Result<UserContext> {
        let claims = self.validator.validate(token).await?;

        // Extract roles and permissions from Auth0 custom claims
        let roles = claims.custom.get("https://fraiseql.com/roles")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect())
            .unwrap_or_default();

        let permissions = claims.custom.get("https://fraiseql.com/permissions")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect())
            .unwrap_or_default();

        Ok(UserContext {
            user_id: Some(claims.sub),
            permissions,
            roles,
        })
    }
}

/// Custom JWT provider (for self-hosted auth)
pub struct CustomJWTProvider {
    validator: JWTValidator,
}

impl CustomJWTProvider {
    pub fn new(issuer: String, audience: Vec<String>, jwks_url: String) -> Self {
        Self {
            validator: JWTValidator::new(issuer, audience, jwks_url),
        }
    }
}

#[async_trait]
impl AuthProvider for CustomJWTProvider {
    async fn validate_token(&self, token: &str) -> Result<UserContext> {
        let claims = self.validator.validate(token).await?;

        // Extract roles/permissions from custom claims
        let roles = claims.custom.get("roles")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect())
            .unwrap_or_default();

        let permissions = claims.custom.get("permissions")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect())
            .unwrap_or_default();

        Ok(UserContext {
            user_id: Some(claims.sub),
            permissions,
            roles,
        })
    }
}
```

### Step 3: User Context Cache (cache.rs)

```rust
//! User context caching with LRU eviction.

use lru::LruCache;
use std::sync::Mutex;
use std::num::NonZeroUsize;
use crate::pipeline::unified::UserContext;

/// User context cache (token -> UserContext)
pub struct UserContextCache {
    cache: Mutex<LruCache<String, (UserContext, u64)>>,  // (context, exp_timestamp)
}

impl UserContextCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(capacity).unwrap())),
        }
    }

    /// Get cached user context if valid
    pub fn get(&self, token_hash: &str) -> Option<UserContext> {
        let mut cache = self.cache.lock().unwrap();

        if let Some((context, exp)) = cache.get(token_hash) {
            // Check if cached context is still valid
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now < *exp {
                return Some(context.clone());
            }

            // Expired - remove from cache
            cache.pop(token_hash);
        }

        None
    }

    /// Cache user context with expiration
    pub fn set(&self, token_hash: String, context: UserContext, exp: u64) {
        let mut cache = self.cache.lock().unwrap();
        cache.put(token_hash, (context, exp));
    }

    /// Clear entire cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
}

/// Hash token for cache key (SHA256)
pub fn hash_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

### Step 4: Auth Errors (errors.rs)

```rust
//! Authentication error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Missing authorization header")]
    MissingAuthHeader,

    #[error("Invalid authorization header format")]
    InvalidAuthHeader,

    #[error("JWKS fetch failed: {0}")]
    JWKSFetchError(String),

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Insufficient permissions")]
    InsufficientPermissions,

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
}
```

### Step 5: Python Wrapper (src/fraiseql/auth/rust_provider.py)

```python
"""Rust-based authentication provider (Python wrapper)."""

from typing import Any

from fraiseql._fraiseql_rs import PyAuthProvider, PyUserContext
from fraiseql.auth.base import AuthProvider, UserContext, AuthenticationError


class RustAuth0Provider(AuthProvider):
    """Auth0 provider using Rust implementation.

    This is 5-10x faster than the Python implementation.
    """

    def __init__(self, domain: str, audience: list[str]):
        self._rust_provider = PyAuthProvider.auth0(domain, audience)

    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate token using Rust."""
        try:
            py_context = await self._rust_provider.validate_token(token)
            return {
                "sub": py_context.user_id,
                "roles": py_context.roles,
                "permissions": py_context.permissions,
            }
        except Exception as e:
            raise AuthenticationError(str(e))

    async def get_user_from_token(self, token: str) -> UserContext:
        """Get user context from token using Rust."""
        try:
            py_context = await self._rust_provider.validate_token(token)
            return UserContext(
                user_id=py_context.user_id,
                roles=py_context.roles,
                permissions=py_context.permissions,
            )
        except Exception as e:
            raise AuthenticationError(str(e))


class RustJWTProvider(AuthProvider):
    """Custom JWT provider using Rust implementation."""

    def __init__(self, issuer: str, audience: list[str], jwks_url: str):
        self._rust_provider = PyAuthProvider.jwt(issuer, audience, jwks_url)

    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate token using Rust."""
        try:
            py_context = await self._rust_provider.validate_token(token)
            return {
                "sub": py_context.user_id,
                "roles": py_context.roles,
                "permissions": py_context.permissions,
            }
        except Exception as e:
            raise AuthenticationError(str(e))

    async def get_user_from_token(self, token: str) -> UserContext:
        """Get user context from token using Rust."""
        try:
            py_context = await self._rust_provider.validate_token(token)
            return UserContext(
                user_id=py_context.user_id,
                roles=py_context.roles,
                permissions=py_context.permissions,
            )
        except Exception as e:
            raise AuthenticationError(str(e))
```

### Step 6: Integration with Unified Pipeline (unified.rs)

```rust
// Add auth validation to execute_sync()

pub fn execute_sync(
    &self,
    query_string: &str,
    variables: HashMap<String, JsonValue>,
    user_context: UserContext,  // Already validated by auth middleware
    auth_required: bool,
) -> Result<Vec<u8>> {
    // Check authentication if required
    if auth_required && user_context.user_id.is_none() {
        return Err(anyhow!("Authentication required"));
    }

    // Phase 6: Parse GraphQL query
    let parsed_query = crate::graphql::parser::parse_query(query_string)?;

    // ... rest of pipeline ...
}
```

### Step 7: PyO3 Bindings (lib.rs)

```rust
// Add to lib.rs

#[pyclass]
pub struct PyAuthProvider {
    provider: Arc<dyn AuthProvider>,
}

#[pymethods]
impl PyAuthProvider {
    /// Create Auth0 provider
    #[staticmethod]
    pub fn auth0(domain: String, audience: Vec<String>) -> Self {
        Self {
            provider: Arc::new(auth::provider::Auth0Provider::new(&domain, audience)),
        }
    }

    /// Create custom JWT provider
    #[staticmethod]
    pub fn jwt(issuer: String, audience: Vec<String>, jwks_url: String) -> Self {
        Self {
            provider: Arc::new(auth::provider::CustomJWTProvider::new(issuer, audience, jwks_url)),
        }
    }

    /// Validate token and return user context
    pub fn validate_token(&self, py: Python, token: String) -> PyResult<PyObject> {
        // Async validation wrapped for Python
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let context = self.provider.validate_token(&token)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            Ok(context)
        })
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PyUserContext {
    #[pyo3(get)]
    pub user_id: Option<String>,
    #[pyo3(get)]
    pub roles: Vec<String>,
    #[pyo3(get)]
    pub permissions: Vec<String>,
}

// Add to module registration
fn fraiseql_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // ... existing exports ...

    m.add_class::<PyAuthProvider>()?;
    m.add_class::<PyUserContext>()?;

    Ok(())
}
```

---

## Verification Commands

### Build Rust Extension

```bash
cd fraiseql_rs
cargo build --release
cd ..
maturin develop --release
```

### Run Auth Tests

```bash
# Unit tests (Rust)
cargo test --lib auth

# Integration tests (Python)
pytest tests/test_rust_auth.py -xvs

# Auth enforcement tests
pytest tests/integration/auth/test_auth_enforcement.py -xvs

# Performance benchmark
pytest tests/performance/test_auth_performance.py -xvs
```

### Expected Test Output

```
tests/test_rust_auth.py::test_auth0_validation ✓ (2ms)
tests/test_rust_auth.py::test_jwt_validation ✓ (1ms)
tests/test_rust_auth.py::test_cached_validation ✓ (<1ms)
tests/test_rust_auth.py::test_expired_token ✓
tests/test_rust_auth.py::test_invalid_token ✓

Performance:
- First token validation: ~5ms (JWKS fetch)
- Cached validation: <1ms (10x faster than Python)
- Cache hit rate: >95% for repeated tokens
```

---

## Acceptance Criteria

**Functionality:**

- ✅ JWT token validation with JWKS support
- ✅ Auth0 provider implementation
- ✅ Custom JWT provider implementation
- ✅ User context caching with LRU eviction
- ✅ Proper error handling (TokenExpired, InvalidToken, etc.)
- ✅ Python wrapper maintains backward compatibility

**Performance:**

- ✅ JWT validation: <1ms (cached), <10ms (uncached)
- ✅ 5-10x faster than Python implementation
- ✅ Cache hit rate >95% for production workloads
- ✅ JWKS cache reduces external API calls

**Testing:**

- ✅ All existing auth tests pass
- ✅ New Rust unit tests for JWT validation
- ✅ Integration tests for Auth0 and custom JWT
- ✅ Performance benchmarks show improvement
- ✅ Error handling tests (expired, invalid, missing tokens)

**Quality:**

- ✅ No compilation warnings
- ✅ No clippy warnings
- ✅ Proper error propagation
- ✅ Thread-safe caching
- ✅ Documentation for all public APIs

---

## DO NOT

❌ **DO NOT** change the Python auth interface (maintain backward compatibility)
❌ **DO NOT** implement rate limiting here (Phase 11)
❌ **DO NOT** implement RBAC permission resolution here (Phase 11)
❌ **DO NOT** add complex auth flows (OAuth2 flows, SAML, etc.) - focus on JWT validation
❌ **DO NOT** implement token refresh/revoke in Phase 10 (nice-to-have for later)
❌ **DO NOT** add database lookups for user data (use JWT claims only)

---

## Dependencies (Cargo.toml)

```toml
[dependencies]
# Existing dependencies...

# Auth dependencies (Phase 10)
jsonwebtoken = "9.2"
reqwest = { version = "0.11", features = ["json"] }
sha2 = "0.10"
lru = "0.12"
async-trait = "0.1"
thiserror = "1.0"
```

---

## Migration Strategy

**Phase 1: Add Rust Auth (Week 1)**

- Implement Rust JWT validation
- Add Auth0Provider and CustomJWTProvider
- Add caching layer

**Phase 2: Python Wrapper (Week 1)**

- Create RustAuth0Provider wrapper
- Maintain backward compatibility
- Add integration tests

**Phase 3: Gradual Migration (Week 2)**

- Update FastAPI to use Rust auth by default
- Keep Python auth as fallback
- Monitor performance improvements

**Phase 4: Deprecation (Week 3+)**

- Deprecate Python auth implementations
- Remove after 2 releases
- Full Rust auth in production

---

## Performance Expectations

**Before (Python):**

- JWT validation: ~5-10ms
- No caching (every request validates)
- Python PyJWT overhead

**After (Rust):**

- First validation: ~5ms (JWKS fetch)
- Cached validation: <1ms
- 5-10x improvement overall
- Reduced memory usage

**Real-World Impact:**

- 1000 req/s → Auth overhead: 1s/s → 0.1s/s
- P99 latency: -4-9ms
- CPU usage: -10-20%

---

## Next Phase Preview

**Phase 11** will add:

- RBAC permission resolution in Rust
- Role hierarchy computation
- PostgreSQL-backed permission caching
- Field-level authorization enforcement
