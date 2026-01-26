# Phase 11.5: High - CSRF Distributed Systems Security

**Priority**: 🟠 HIGH
**CVSS Score**: 7.5
**Effort**: 6 hours
**Duration**: 1-2 days
**Status**: [ ] Not Started

---

## Objective

Fix CSRF token validation across distributed deployments by replacing in-memory state store with persistent backend (Redis/PostgreSQL).

---

## Success Criteria

- [ ] Persistent state store implemented (Redis)
- [ ] State tokens work across multiple instances
- [ ] TTL-based expiration working
- [ ] Single-instance fallback to in-memory
- [ ] Integration tests with multi-instance setup
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## Vulnerability Details

**Location**: `crates/fraiseql-server/src/auth/handlers.rs:25`

**Risk**: In-memory state store doesn't work with load-balanced deployments. OAuth callback routed to different instance than CSRF generation → validation fails → CSRF check bypassed.

---

## Implementation Plan

### TDD Cycle 1: Redis State Store

#### RED: Write test for persistent state
```rust
#[tokio::test]
async fn test_state_persists_across_instances() {
    let redis = redis::connect("redis://localhost").await.unwrap();

    // Instance 1: Create state
    let state = OAuthState::create(&redis).await.unwrap();

    // Instance 2: Validate state (different process)
    let valid = OAuthState::validate(&redis, &state.token).await.unwrap();
    assert!(valid);
}

#[test]
fn test_state_expires_after_ttl() {
    // State should expire after 10 minutes
    // Implementation uses Redis EXPIREAT
}
```

#### GREEN: Implement Redis state store
```rust
pub struct RedisStateStore {
    client: redis::Client,
}

impl RedisStateStore {
    pub async fn create_state(&self, nonce: String) -> Result<OAuthState> {
        let state = generate_random_string(32);
        let key = format!("oauth:state:{}", state);

        // Store in Redis with 10-minute expiration
        self.client
            .set_ex(&key, &nonce, 600)  // 600 seconds = 10 minutes
            .await?;

        Ok(OAuthState {
            token: state,
            nonce,
        })
    }

    pub async fn validate_state(
        &self,
        token: &str,
        expected_nonce: &str,
    ) -> Result<bool> {
        let key = format!("oauth:state:{}", token);

        let stored_nonce: Option<String> = self.client.get(&key).await?;

        if let Some(nonce) = stored_nonce {
            // Delete after use (prevent replay)
            self.client.delete(&key).await?;

            Ok(nonce == expected_nonce)
        } else {
            Ok(false)
        }
    }
}
```

#### REFACTOR: Create StateStore trait
```rust
#[async_trait]
pub trait StateStore: Send + Sync {
    async fn create_state(&self, nonce: String) -> Result<OAuthState>;
    async fn validate_state(&self, token: &str, expected_nonce: &str)
        -> Result<bool>;
}

#[async_trait]
impl StateStore for RedisStateStore {
    // Implementation above
}
```

#### CLEANUP
- [ ] All tests pass
- [ ] Redis connection error handling
- [ ] Clippy passes

---

### TDD Cycle 2: In-Memory Fallback

#### RED: Write test for single-instance fallback
```rust
#[test]
fn test_in_memory_store_works_for_single_instance() {
    let store = InMemoryStateStore::new();

    // Create and validate in same instance
    let state = store.create_state("nonce123").await.unwrap();
    let valid = store.validate_state(&state.token, "nonce123").await.unwrap();

    assert!(valid);
}
```

#### GREEN: Implement in-memory fallback
```rust
pub struct InMemoryStateStore {
    states: Arc<DashMap<String, (String, Instant)>>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self {
            states: Arc::new(DashMap::new()),
        }
    }

    // Periodically clean expired states
    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                self.cleanup_expired();
            }
        });
    }

    fn cleanup_expired(&self) {
        let now = Instant::now();
        self.states.retain(|_, (_, created)| {
            now.duration_since(*created) < Duration::from_secs(600)
        });
    }
}

#[async_trait]
impl StateStore for InMemoryStateStore {
    async fn create_state(&self, nonce: String) -> Result<OAuthState> {
        let state = generate_random_string(32);
        self.states.insert(state.clone(), (nonce, Instant::now()));
        Ok(OAuthState { token: state, nonce })
    }

    async fn validate_state(
        &self,
        token: &str,
        expected_nonce: &str,
    ) -> Result<bool> {
        if let Some((nonce, created)) = self.states.remove(token) {
            let elapsed = Instant::now().duration_since(created);
            Ok(nonce == expected_nonce && elapsed < Duration::from_secs(600))
        } else {
            Ok(false)
        }
    }
}
```

#### REFACTOR: Create factory for state store selection
```rust
pub fn create_state_store(config: &AppConfig) -> Arc<dyn StateStore> {
    match &config.oauth_state_backend {
        StateBackendConfig::Redis { url } => {
            let client = redis::Client::open(url).unwrap();
            Arc::new(RedisStateStore { client })
        }
        StateBackendConfig::InMemory => {
            let store = Arc::new(InMemoryStateStore::new());
            store.clone().start_cleanup_task();
            store as Arc<dyn StateStore>
        }
    }
}
```

#### CLEANUP
- [ ] Tests pass
- [ ] Cleanup tasks working
- [ ] No memory leaks

---

### TDD Cycle 3: Integration with OAuth Flow

#### RED: Write multi-instance integration test
```rust
#[tokio::test]
async fn test_oauth_works_across_instances() {
    let redis = redis::connect("redis://localhost").await.unwrap();
    let store = Arc::new(RedisStateStore { client: redis });

    // Instance 1: Generate state
    let state = store.create_state("nonce123".to_string()).await.unwrap();

    // Instance 2: Validate state (simulated different server)
    let valid = store
        .validate_state(&state.token, "nonce123")
        .await
        .unwrap();

    assert!(valid);
}
```

#### GREEN: Update OAuth handlers
```rust
pub async fn handle_oauth_callback(
    state: &str,
    code: &str,
    store: Arc<dyn StateStore>,
) -> Result<AuthToken> {
    // Extract nonce from query parameter
    let nonce = get_nonce_from_request();

    // Validate state (works across instances)
    let valid = store.validate_state(state, &nonce).await?;

    if !valid {
        return Err(Error::CsrfValidationFailed);
    }

    // Continue with OAuth flow
    exchange_code_for_token(code).await
}
```

#### REFACTOR: Dependency injection of store
```rust
pub struct OAuthService {
    store: Arc<dyn StateStore>,
    // ... other fields
}

impl OAuthService {
    pub fn new(store: Arc<dyn StateStore>) -> Self {
        Self { store, .. }
    }

    pub async fn handle_callback(&self, state: &str, code: &str) -> Result<AuthToken> {
        // Use injected store
        // ...
    }
}
```

#### CLEANUP
- [ ] All tests pass
- [ ] Integration verified
- [ ] Clippy passes

---

## Files to Modify

1. **`Cargo.toml`**
   - Add `redis` dependency
   - Add `async-trait` if not present

2. **`crates/fraiseql-server/src/auth/state_store.rs`** (new)
   - StateStore trait
   - RedisStateStore implementation
   - InMemoryStateStore implementation

3. **`crates/fraiseql-server/src/auth/handlers.rs`**
   - Inject state store dependency
   - Use state store in OAuth handlers

4. **`crates/fraiseql-server/src/config.rs`**
   - Add StateBackendConfig
   - Factory function for state store creation

---

## Tests to Create

```rust
#[cfg(test)]
mod csrf_security_tests {
    use super::*;

    // Redis tests
    #[tokio::test]
    async fn test_redis_state_persists() { }

    #[tokio::test]
    async fn test_redis_state_expires() { }

    // In-memory tests
    #[test]
    fn test_in_memory_state_works() { }

    #[test]
    fn test_in_memory_cleanup() { }

    // Integration tests
    #[tokio::test]
    async fn test_oauth_multi_instance() { }

    #[tokio::test]
    async fn test_expired_state_rejected() { }

    #[tokio::test]
    async fn test_used_state_not_replayable() { }
}
```

---

## Configuration

```toml
[oauth]
# State backend: redis or memory
state_backend = "redis"
redis_url = "redis://localhost:6379"

# For single-instance deployment:
# state_backend = "memory"
```

---

## Dependencies Added

```toml
redis = { version = "0.24", features = ["aio", "tokio-comp"] }
async-trait = "0.1"
```

---

## Performance Impact

**Expected**: Minimal
- Redis latency: <5ms for state operations
- In-memory: <1ms
- Cleanup: background task, minimal overhead

---

## Commit Message Template

```
fix(security-11.5): Fix CSRF in distributed deployments

## Changes
- Replace in-memory CSRF state store with persistent backend
- Add RedisStateStore for multi-instance deployments
- Keep InMemoryStateStore for single-instance fallback
- Implement automatic state expiration (10 minutes)
- Add background cleanup for in-memory store

## Vulnerability Addressed
CVSS 7.5 - CSRF in distributed systems

## Verification
✅ Multi-instance OAuth tests pass
✅ State persistence verified
✅ Expiration working
✅ Clippy clean
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN TDD CYCLE 1 - Redis state store

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
