# Phase 10 QA Findings & Corrections

**Date**: December 21, 2024
**Phase**: Authentication & Token Validation in Rust
**Status**: ⚠️ Issues Found - Corrections Required

---

## ✅ What's Good

### Architecture

- ✅ Clean separation of concerns (jwt.rs, provider.rs, cache.rs, errors.rs)
- ✅ Trait-based provider design allows multiple auth backends
- ✅ UserContext struct already exists in unified.rs (Phase 9)
- ✅ LRU caching strategy is sound
- ✅ JWKS caching with TTL is correct approach

### Design Patterns

- ✅ Async/await properly used throughout
- ✅ Error handling with Result<T> and custom error types
- ✅ Thread-safe caching with Arc<Mutex<>>
- ✅ Python wrapper maintains backward compatibility

### Dependencies

- ✅ jsonwebtoken 9.2 is correct version
- ✅ reqwest for JWKS fetching is appropriate
- ✅ sha2 for token hashing is correct
- ✅ lru for caching is standard
- ✅ async-trait for trait async methods

---

## ❌ Critical Issues Found

### Issue 1: Missing JWK to PEM Conversion Implementation

**Location**: `jwt.rs:193-198`

**Problem**:

```rust
fn jwk_to_pem(jwk: &JWK) -> Result<String> {
    // Convert JWK (n, e) to PEM format
    // Implementation uses base64 decoding + ASN.1 encoding
    // (Simplified for phase plan - full implementation needed)
    todo!("Implement JWK to PEM conversion")
}
```

**Impact**: Critical - JWT validation will panic on first use

**Solution**: Use existing crate instead of manual implementation

**Fix Required**:

```toml
# Add to Cargo.toml
[dependencies]
jsonwebkey = "0.3"  # Handles JWK to PEM conversion
base64 = "0.21"
```

```rust
// Replace jwt.rs JWK handling with:
use jsonwebkey as jwk;

fn jwk_to_pem(jwk: &JWK) -> Result<String> {
    // Use jsonwebkey crate for proper conversion
    let key = jwk::JsonWebKey::from_str(&serde_json::to_string(jwk)?)?;
    let pem = key.key.to_pem();
    Ok(pem)
}

// OR better: Use jsonwebtoken's built-in JWK support
// jsonwebtoken 9.0+ has DecodingKey::from_jwk()
use jsonwebtoken::jwk::JwkSet;

// Modify fetch_jwks to return proper type:
async fn fetch_jwks(&self, url: &str) -> Result<JwkSet> {
    let response = reqwest::get(url).await?;
    let jwks: JwkSet = response.json().await?;
    Ok(jwks)
}

// Then use:
let decoding_key = DecodingKey::from_jwk(&jwk)?;
```

**Recommendation**: Use jsonwebtoken's built-in JWK support (simpler, more reliable)

---

### Issue 2: Missing SystemTime Import

**Location**: `jwt.rs:129,146`

**Problem**:

```rust
cache: Arc<Mutex<HashMap<String, (String, SystemTime)>>>,  // SystemTime not imported
```

**Impact**: Compilation error

**Fix**:

```rust
use std::time::SystemTime;
```

---

### Issue 3: Missing Arc Import in jwt.rs

**Location**: `jwt.rs:129`

**Problem**:

```rust
cache: Arc<Mutex<HashMap<String, (String, SystemTime)>>>,  // Arc not imported
```

**Impact**: Compilation error

**Fix**:

```rust
use std::sync::{Arc, Mutex};
```

---

### Issue 4: Incorrect PyO3 Async Integration

**Location**: `lib.rs:543-552`

**Problem**:

```rust
pub fn validate_token(&self, py: Python, token: String) -> PyResult<PyObject> {
    // Async validation wrapped for Python
    pyo3_asyncio::tokio::future_into_py(py, async move {
        let context = self.provider.validate_token(&token)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(context)  // ❌ Wrong: context is UserContext, not PyObject
    })
}
```

**Impact**: Type mismatch - won't compile

**Fix**:

```rust
pub fn validate_token(&self, py: Python, token: String) -> PyResult<PyObject> {
    let provider = self.provider.clone();

    pyo3_asyncio::tokio::future_into_py(py, async move {
        let context = provider.validate_token(&token)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        // Convert UserContext to PyUserContext
        Python::with_gil(|py| {
            let py_context = PyUserContext {
                user_id: context.user_id,
                roles: context.roles,
                permissions: context.permissions,
            };
            Ok(py_context.into_py(py))
        })
    })
}
```

---

### Issue 5: UserContext Not Implementing Clone

**Location**: `cache.rs:347`

**Problem**:

```rust
return Some(context.clone());  // UserContext doesn't derive Clone
```

**Impact**: Compilation error

**Fix in unified.rs**:

```rust
/// User context for authorization and personalization.
#[derive(Debug, Clone)]  // ✅ Add Clone
pub struct UserContext {
    pub user_id: Option<String>,
    pub permissions: Vec<String>,
    pub roles: Vec<String>,
}
```

---

### Issue 6: Missing Dependency - pyo3-asyncio

**Location**: `lib.rs:545`

**Problem**: Uses `pyo3_asyncio::tokio::future_into_py` but dependency not listed

**Impact**: Compilation error

**Fix in Cargo.toml**:

```toml
[dependencies]
pyo3-asyncio = { version = "0.21", features = ["tokio-runtime"] }
tokio = { version = "1.35", features = ["full"] }
```

**Note**: Phase 10 plan shows this but needs to be explicit in the dependencies section

---

## ⚠️ Medium Priority Issues

### Issue 7: JWKS Cache Key Collision Risk

**Location**: `jwt.rs:140-171`

**Problem**: Cache uses `kid` as key, but different JWKS URLs might have same `kid`

**Risk**: Medium - unlikely in practice but possible

**Fix**:

```rust
// Use composite key: (jwks_url, kid)
cache: Arc<Mutex<HashMap<(String, String), (String, SystemTime)>>>,

// In get_key:
let cache_key = (jwks_url.to_string(), kid.to_string());
if let Some((key, cached_at)) = cache.get(&cache_key) {
    // ...
}
```

---

### Issue 8: Token Hash Collision Risk

**Location**: `cache.rs:371-376`

**Problem**: Uses SHA256 hash for cache key, but stores full token would be safer

**Risk**: Low - SHA256 collisions are astronomically unlikely

**Consideration**: Hashing is correct for security (don't store raw tokens in cache)

**Recommendation**: Keep as-is, but document that this is intentional

---

### Issue 9: Missing JWKS Fetch Timeout

**Location**: `jwt.rs:173-177`

**Problem**:

```rust
async fn fetch_jwks(&self, url: &str) -> Result<JWKS> {
    let response = reqwest::get(url).await?;  // No timeout
    let jwks: JWKS = response.json().await?;
    Ok(jwks)
}
```

**Risk**: Hanging requests if Auth0/JWKS endpoint is slow

**Fix**:

```rust
async fn fetch_jwks(&self, url: &str) -> Result<JWKS> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let response = client.get(url).send().await?;
    let jwks: JWKS = response.json().await?;
    Ok(jwks)
}
```

---

### Issue 10: No Cache Size Limit on JWKS Cache

**Location**: `jwt.rs:128-137`

**Problem**: HashMap grows unbounded as new `kid` values are added

**Risk**: Memory leak in long-running processes

**Fix**: Use LRU cache instead of HashMap

```rust
use lru::LruCache;
use std::num::NonZeroUsize;

struct JWKSCache {
    cache: Arc<Mutex<LruCache<String, (String, SystemTime)>>>,
}

impl JWKSCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(100).unwrap())  // Max 100 keys
            )),
        }
    }
}
```

---

## ℹ️ Minor Issues / Suggestions

### Issue 11: Missing Documentation on Auth0 Custom Claims

**Location**: `provider.rs:248-261`

**Suggestion**: Document the expected Auth0 custom claim format

**Fix**: Add documentation:

```rust
/// Extract roles and permissions from Auth0 custom claims
///
/// Expected Auth0 custom claims:
/// - `https://fraiseql.com/roles`: Array of role names
/// - `https://fraiseql.com/permissions`: Array of permission strings
///
/// Example JWT payload:
/// ```json
/// {
///   "sub": "auth0|123456",
///   "https://fraiseql.com/roles": ["admin", "user"],
///   "https://fraiseql.com/permissions": ["posts:write", "users:read"]
/// }
/// ```
```

---

### Issue 12: Missing Audience Validation Error Details

**Location**: `jwt.rs:115`

**Suggestion**: Improve error message when audience validation fails

**Current**:

```rust
validation.set_audience(&self.audience);
```

**Better**:

```rust
validation.set_audience(&self.audience);
// jsonwebtoken will return generic error
// Consider wrapping with better message:
let token_data = decode::<Claims>(token, &decoding_key, &validation)
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::InvalidAudience => {
            anyhow!("Invalid audience. Expected: {:?}, Got token for different audience", self.audience)
        }
        _ => anyhow!("JWT validation failed: {}", e)
    })?;
```

---

### Issue 13: Missing exp Claim Validation in Cache

**Location**: `cache.rs:336-354`

**Suggestion**: Extract `exp` from JWT claims instead of passing separately

**Current**:

```rust
pub fn set(&self, token_hash: String, context: UserContext, exp: u64) {
```

**Issue**: Caller must extract `exp` - duplicated logic

**Better**: Extract from JWT during validation and include in UserContext

```rust
// In UserContext (unified.rs):
pub struct UserContext {
    pub user_id: Option<String>,
    pub permissions: Vec<String>,
    pub roles: Vec<String>,
    pub exp: u64,  // Add expiration timestamp
}

// Then cache.rs just uses context.exp
```

---

### Issue 14: Race Condition in JWKS Cache Check-Then-Act

**Location**: `jwt.rs:141-151`

**Problem**: Check cache, release lock, fetch JWKS, re-acquire lock
Another thread might fetch the same key in parallel

**Risk**: Low - wasteful but not dangerous (both will cache same result)

**Fix**: Use a more sophisticated cache with built-in fetch-if-missing

```rust
// Use moka crate with async support
use moka::future::Cache;

struct JWKSCache {
    cache: Cache<String, String>,  // kid -> PEM
}

impl JWKSCache {
    pub async fn get_or_fetch(&self, kid: &str, url: &str) -> Result<String> {
        self.cache.try_get_with(kid.to_string(), async {
            self.fetch_and_convert(kid, url).await
        }).await.map_err(|e| anyhow!("Cache error: {}", e))
    }
}
```

---

## 🔒 Security Considerations

### Security 1: Token Storage in Cache

**Status**: ✅ Good - tokens are hashed with SHA256 before caching

**Verification**:

```rust
pub fn hash_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

**Recommendation**: Keep as-is. Never store raw JWT tokens in cache.

---

### Security 2: JWKS Fetch Over HTTPS

**Status**: ⚠️ Should validate HTTPS

**Issue**: `reqwest::get(url)` accepts HTTP URLs

**Fix**:

```rust
async fn fetch_jwks(&self, url: &str) -> Result<JWKS> {
    // Validate HTTPS
    if !url.starts_with("https://") {
        return Err(anyhow!("JWKS URL must use HTTPS: {}", url));
    }

    let response = reqwest::get(url).await?;
    let jwks: JWKS = response.json().await?;
    Ok(jwks)
}
```

---

### Security 3: Algorithm Restriction

**Status**: ✅ Good - hardcoded to RS256

**Verification**:

```rust
algorithms: vec![Algorithm::RS256],
```

**Recommendation**: Keep as-is. Don't allow HS256 for Auth0.

---

## 📝 Required Changes Summary

### Must Fix (Compilation Errors)

1. ✅ Add missing imports: `SystemTime`, `Arc`, `Mutex`
2. ✅ Implement JWK to PEM conversion (use `jsonwebtoken::DecodingKey::from_jwk`)
3. ✅ Fix PyO3 async return type (convert UserContext → PyUserContext)
4. ✅ Add `Clone` derive to UserContext
5. ✅ Add `pyo3-asyncio` dependency

### Should Fix (Runtime Issues)

6. ✅ Add JWKS fetch timeout (5 seconds)
7. ✅ Use LRU cache for JWKS (prevent unbounded growth)
8. ✅ Fix JWKS cache key collision (use composite key)
9. ✅ Validate HTTPS for JWKS URLs

### Nice to Have (Improvements)

10. ℹ️ Document Auth0 custom claims format
11. ℹ️ Better error messages for audience validation
12. ℹ️ Include `exp` in UserContext (avoid duplicate extraction)
13. ℹ️ Use `moka` cache to prevent race conditions

---

## 🔧 Corrected Implementation

### Corrected jwt.rs (Key Changes)

```rust
//! JWT token validation with Auth0/custom JWKS support.

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use jsonwebtoken::jwk::{JwkSet, Jwk};  // ✅ Use built-in JWK support
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};  // ✅ Add Arc, Mutex
use std::time::{SystemTime, Duration};  // ✅ Add SystemTime
use lru::LruCache;  // ✅ Use LRU instead of HashMap
use std::num::NonZeroUsize;

/// JWT claims structure (Auth0 compatible)
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub exp: usize,  // ✅ Keep for cache expiry
    pub iat: usize,
    pub iss: String,
    pub aud: Vec<String>,

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
    http_client: reqwest::Client,  // ✅ Reuse HTTP client
}

impl JWTValidator {
    pub fn new(issuer: String, audience: Vec<String>, jwks_url: String) -> Result<Self> {
        // ✅ Validate HTTPS
        if !jwks_url.starts_with("https://") {
            return Err(anyhow!("JWKS URL must use HTTPS"));
        }

        // ✅ Create HTTP client with timeout
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;

        Ok(Self {
            issuer,
            audience,
            jwks_url,
            jwks_cache: JWKSCache::new(),
            algorithms: vec![Algorithm::RS256],
            http_client,
        })
    }

    pub async fn validate(&self, token: &str) -> Result<Claims> {
        let header = decode_header(token)?;
        let kid = header.kid.ok_or_else(|| anyhow!("Missing kid in token header"))?;

        // ✅ Get JWK from cache
        let jwk = self.jwks_cache.get_jwk(&kid, &self.jwks_url, &self.http_client).await?;

        // ✅ Use jsonwebtoken's built-in JWK support
        let decoding_key = DecodingKey::from_jwk(&jwk)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&self.audience);

        let token_data = decode::<Claims>(token, &decoding_key, &validation)?;

        Ok(token_data.claims)
    }
}

/// JWKS cache with LRU eviction and 1-hour TTL
struct JWKSCache {
    cache: Arc<Mutex<LruCache<String, (Jwk, SystemTime)>>>,  // ✅ LRU instead of HashMap
}

impl JWKSCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(100).unwrap())  // ✅ Max 100 keys
            )),
        }
    }

    /// Get JWK by kid (fetches from JWKS if not cached)
    pub async fn get_jwk(
        &self,
        kid: &str,
        jwks_url: &str,
        client: &reqwest::Client,
    ) -> Result<Jwk> {
        // Check cache
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some((jwk, cached_at)) = cache.get(kid) {
                let elapsed = SystemTime::now().duration_since(*cached_at)?;
                if elapsed.as_secs() < 3600 {
                    return Ok(jwk.clone());
                }
                // Expired - remove
                cache.pop(kid);
            }
        }

        // Fetch JWKS
        let jwks = self.fetch_jwks(jwks_url, client).await?;

        // Find key
        let jwk = jwks.keys.iter()
            .find(|k| k.common.key_id.as_ref() == Some(&kid.to_string()))
            .ok_or_else(|| anyhow!("Key not found: {}", kid))?
            .clone();

        // Cache it
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(kid.to_string(), (jwk.clone(), SystemTime::now()));
        }

        Ok(jwk)
    }

    async fn fetch_jwks(&self, url: &str, client: &reqwest::Client) -> Result<JwkSet> {
        let response = client.get(url).send().await?;
        let jwks: JwkSet = response.json().await?;
        Ok(jwks)
    }
}
```

### Corrected unified.rs (UserContext)

```rust
/// User context for authorization and personalization.
#[derive(Debug, Clone)]  // ✅ Add Clone
pub struct UserContext {
    pub user_id: Option<String>,
    pub permissions: Vec<String>,
    pub roles: Vec<String>,
    pub exp: u64,  // ✅ Add expiration for cache
}
```

### Corrected lib.rs (PyO3 Bindings)

```rust
#[pymethods]
impl PyAuthProvider {
    pub fn validate_token(&self, py: Python, token: String) -> PyResult<PyObject> {
        let provider = self.provider.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let context = provider.validate_token(&token)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            // ✅ Convert to PyUserContext
            Python::with_gil(|py| {
                let py_context = PyUserContext {
                    user_id: context.user_id,
                    roles: context.roles,
                    permissions: context.permissions,
                };
                Ok(Py::new(py, py_context)?.into_py(py))
            })
        })
    }
}
```

---

## ✅ Updated Dependencies

```toml
[dependencies]
# Existing dependencies...

# Auth dependencies (Phase 10)
jsonwebtoken = "9.2"  # ✅ Has built-in JWK support
reqwest = { version = "0.11", features = ["json"] }
sha2 = "0.10"
lru = "0.12"
async-trait = "0.1"
thiserror = "1.0"
pyo3-asyncio = { version = "0.21", features = ["tokio-runtime"] }  # ✅ Add
tokio = { version = "1.35", features = ["full"] }  # ✅ Ensure full features
```

---

## 📊 QA Summary

| Category | Issues Found | Critical | Medium | Minor |
|----------|--------------|----------|--------|-------|
| Compilation Errors | 5 | 5 | 0 | 0 |
| Runtime Issues | 4 | 0 | 4 | 0 |
| Security | 2 | 0 | 1 | 1 |
| Improvements | 3 | 0 | 0 | 3 |
| **Total** | **14** | **5** | **5** | **4** |

**Status**: ⚠️ **Phase 10 requires corrections before implementation**

**Severity**:

- 🔴 **5 Critical**: Must fix (compilation errors)
- 🟡 **5 Medium**: Should fix (runtime issues)
- 🟢 **4 Minor**: Nice to have (improvements)

**Estimated Fix Time**: 2-4 hours

---

## 🎯 Action Items

### Before Implementation

1. ✅ Update jwt.rs with corrected implementation
2. ✅ Add missing imports
3. ✅ Update UserContext to derive Clone and include exp
4. ✅ Fix PyO3 async binding
5. ✅ Update dependencies in Cargo.toml

### During Implementation

6. ✅ Add HTTPS validation for JWKS URLs
7. ✅ Add timeout to JWKS fetching
8. ✅ Use LRU cache for JWKS
9. ✅ Add comprehensive error messages

### After Implementation

10. ✅ Write unit tests for all fixes
11. ✅ Performance benchmark auth validation
12. ✅ Document Auth0 custom claims format
13. ✅ Update phase plan with corrections

---

## 📝 Conclusion

Phase 10 is **architecturally sound** but has **5 critical compilation errors** that must be fixed before implementation. The good news:

✅ **Core design is correct**
✅ **Dependencies are appropriate**
✅ **Security approach is sound**
✅ **All issues are straightforward to fix**

The phase plan is a **solid foundation** - just needs the corrections documented above before implementation can proceed.

**Recommendation**: Apply corrections → Implement → Test → Deploy

---

*QA Completed: December 21, 2024*
*Next Step: Create corrected phase-10-auth-integration-v2.md*
