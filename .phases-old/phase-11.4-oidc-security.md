# Phase 11.4: High - OIDC Token Cache Poisoning Prevention

**Priority**: 🟠 HIGH
**CVSS Score**: 7.8
**Effort**: 4 hours
**Duration**: 1 day
**Status**: [ ] Not Started

---

## Objective

Prevent OIDC token cache poisoning by reducing cache TTL, detecting key rotation, and invalidating cache proactively.

---

## Success Criteria

- [ ] Cache TTL reduced from 3600s to 300s
- [ ] Key rotation monitoring implemented
- [ ] Cache invalidation on key miss added
- [ ] Tests verify key rotation detection
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## Vulnerability Details

**Location**: `crates/fraiseql-core/src/security/oidc.rs:629-667`

**Risk**: 1-hour cache window allows revoked tokens to pass validation after key rotation. Attacker can impersonate users with old tokens for extended period.

---

## Implementation Plan

### TDD Cycle 1: Reduce Cache TTL

#### RED: Write test for TTL
```rust
#[test]
fn test_cache_ttl_is_short() {
    let cache = JwksCache::new();
    assert_eq!(cache.ttl(), Duration::from_secs(300));  // 5 minutes
}
```

#### GREEN: Update TTL constant
```rust
const JWKS_CACHE_TTL: Duration = Duration::from_secs(300);  // 5 min (was 3600)

#[derive(Clone)]
pub struct CachedJwks {
    jwks: JsonWebKeySet,
    cached_at: Instant,
    ttl: Duration,
}

impl CachedJwks {
    pub fn new(jwks: JsonWebKeySet) -> Self {
        Self {
            jwks,
            cached_at: Instant::now(),
            ttl: JWKS_CACHE_TTL,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}
```

#### REFACTOR: Make TTL configurable
```rust
pub struct OidcConfig {
    pub jwks_cache_ttl: Duration,
    // ... other fields
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            jwks_cache_ttl: Duration::from_secs(300),  // 5 min default
            // ...
        }
    }
}
```

#### CLEANUP
- [ ] Verify tests pass
- [ ] Check config defaults
- [ ] Clippy passes

---

### TDD Cycle 2: Detect Key Rotation

#### RED: Write test for key rotation detection
```rust
#[test]
fn test_key_rotation_detected() {
    let mut provider = OidcProvider::new(config);

    // Cache initial JWKS
    let jwks1 = provider.fetch_jwks_blocking().unwrap();
    provider.cache_jwks(jwks1);

    // Simulate key rotation (different keys)
    let jwks2 = JsonWebKeySet::with_kids(vec!["new_key_1", "new_key_2"]);

    // Detect rotation
    assert!(provider.key_rotation_detected(&jwks2));
}
```

#### GREEN: Implement rotation detection
```rust
pub fn key_rotation_detected(&self, new_jwks: &JsonWebKeySet) -> bool {
    let cached = self.jwks_cache.read();

    if let Some(ref cached) = *cached {
        let old_kids: HashSet<_> = cached.jwks.kids().collect();
        let new_kids: HashSet<_> = new_jwks.kids().collect();

        // Check if any old keys are gone
        !old_kids.is_subset(&new_kids)
    } else {
        false
    }
}
```

#### REFACTOR: Add monitoring loop
```rust
pub async fn start_key_rotation_monitor(self: Arc<Self>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            if let Ok(new_jwks) = self.fetch_jwks().await {
                if self.key_rotation_detected(&new_jwks) {
                    warn!("🔄 OIDC key rotation detected!");
                    self.clear_cache();
                    self.cache_jwks(new_jwks);
                }
            }
        }
    });
}
```

#### CLEANUP
- [ ] All tests pass
- [ ] Logging messages clear
- [ ] No performance issues from monitoring

---

### TDD Cycle 3: Cache Invalidation on Key Miss

#### RED: Write test for cache invalidation
```rust
#[test]
async fn test_cache_invalidated_on_key_miss() {
    let provider = OidcProvider::new(config);

    // Cache JWKS with key_1
    let jwks = JsonWebKeySet::with_kids(vec!["key_1"]);
    provider.cache_jwks(jwks);

    // Try to get key_2 (not in cache)
    let result = provider.get_decoding_key("key_2").await;

    // Cache should be invalidated
    assert!(provider.jwks_cache.read().is_none());

    // Fresh JWKS should be fetched
    assert!(result.is_ok());
}
```

#### GREEN: Implement cache invalidation on miss
```rust
pub async fn get_decoding_key(&self, kid: &str) -> Result<DecodingKey> {
    // Check cache
    {
        let cache = self.jwks_cache.read();
        if let Some(ref cached) = *cache {
            if !cached.is_expired() {
                if let Some(key) = self.find_key(&cached.jwks, kid) {
                    return self.jwk_to_decoding_key(key);
                }
                // KEY NOT FOUND - cache likely stale
                drop(cache);  // Release lock
            }
        }
    }

    // Cache miss or expired - fetch fresh JWKS
    let jwks = self.fetch_jwks().await?;
    let key = self.find_key(&jwks, kid)
        .ok_or(Error::JwkNotFound(kid.to_string()))?;

    // Update cache
    let mut cache_write = self.jwks_cache.write();
    *cache_write = Some(CachedJwks::new(jwks));

    self.jwk_to_decoding_key(key)
}
```

#### REFACTOR: Extract cache management
```rust
pub async fn get_key_with_rotation_handling(
    &self,
    kid: &str,
) -> Result<DecodingKey> {
    // Try cache first
    if let Some(key) = self.get_key_from_cache(kid) {
        return Ok(key);
    }

    // Cache miss - fetch fresh
    self.get_key_and_update_cache(kid).await
}

fn get_key_from_cache(&self, kid: &str) -> Option<DecodingKey> {
    let cache = self.jwks_cache.read();
    cache.as_ref()
        .filter(|c| !c.is_expired())
        .and_then(|c| self.find_key(&c.jwks, kid))
        .map(|k| self.jwk_to_decoding_key(k).ok())
        .flatten()
}

async fn get_key_and_update_cache(&self, kid: &str) -> Result<DecodingKey> {
    let jwks = self.fetch_jwks().await?;
    let key = self.find_key(&jwks, kid)
        .ok_or(Error::JwkNotFound(kid.to_string()))?;

    let mut cache_write = self.jwks_cache.write();
    *cache_write = Some(CachedJwks::new(jwks));

    self.jwk_to_decoding_key(key)
}
```

#### CLEANUP
- [ ] All tests pass
- [ ] Cache management is clean
- [ ] No deadlocks from locks

---

## Files to Modify

1. **`crates/fraiseql-core/src/security/oidc.rs`**
   - Reduce JWKS_CACHE_TTL
   - Add key rotation detection
   - Implement cache invalidation on key miss
   - Add monitoring loop

2. **`crates/fraiseql-core/src/config.rs`**
   - Add OidcConfig with configurable TTL
   - Document TTL options

---

## Tests to Create

```rust
#[cfg(test)]
mod oidc_security_tests {
    use super::*;

    // Cache TTL tests
    #[test]
    fn test_cache_ttl_short() { }

    #[test]
    fn test_cache_expires_after_ttl() { }

    // Key rotation tests
    #[tokio::test]
    async fn test_rotation_detected() { }

    #[tokio::test]
    async fn test_rotation_clears_cache() { }

    // Cache invalidation tests
    #[tokio::test]
    async fn test_key_miss_invalidates_cache() { }

    #[tokio::test]
    async fn test_fresh_fetch_on_key_miss() { }

    // Integration tests
    #[tokio::test]
    async fn test_old_token_rejected_after_rotation() { }
}
```

---

## Configuration

```toml
[oidc]
# Cache TTL in seconds (default: 300 = 5 minutes)
jwks_cache_ttl_secs = 300

# Key rotation monitoring interval (default: 30 seconds)
key_rotation_check_interval_secs = 30

# Provider URL
provider_url = "https://accounts.google.com"
```

---

## Performance Impact

**Expected**: Minimal
- More frequent JWKS fetches: 1 per 5 minutes vs 1 per 60 minutes
- Monitoring loop: 1 HTTP request every 30 seconds (background)
- Cache lookups: still O(1)

---

## Commit Message Template

```
fix(security-11.4): Prevent OIDC token cache poisoning

## Changes
- Reduce JWKS cache TTL from 3600s to 300s
- Implement key rotation detection
- Add cache invalidation on key miss
- Start background monitoring of key rotations

## Vulnerability Addressed
CVSS 7.8 - OIDC token cache poisoning

## Verification
✅ TTL tests pass
✅ Key rotation detection works
✅ Cache invalidation on miss works
✅ Clippy clean
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN TDD CYCLE 1 - Reduce cache TTL

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
