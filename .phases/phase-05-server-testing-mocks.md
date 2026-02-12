# Phase 5: Server Testing Mock Implementations

## Objective
Implement mock/in-memory test doubles for `CacheClient`, `RateLimiter`, and
`IdempotencyStore` to enable unit testing of server components without Redis or
other infrastructure.

## Success Criteria
- [ ] `MockCacheClient` implements `CacheClient` trait with in-memory storage + TTL
- [ ] `MockRateLimiter` implements `RateLimiter` trait with per-key tracking
- [ ] `MockIdempotencyStore` implements `IdempotencyStore` trait
- [ ] All mocks are `Send + Sync` (required by trait bounds)
- [ ] Tests for each mock implementation
- [ ] `cargo clippy -p fraiseql-server` clean
- [ ] `cargo test -p fraiseql-server` passes

## Background

### Trait Signatures (from `runtime_state.rs`)

```rust
// CacheClient (lines 117-127)
#[async_trait::async_trait]
pub trait CacheClient: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, RuntimeError>;
    async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<(), RuntimeError>;
    async fn delete(&self, key: &str) -> Result<(), RuntimeError>;
    async fn ping(&self) -> Result<(), RuntimeError>;
}

// RateLimiter (lines 131-138)
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<RateLimitResult, RuntimeError>;
}

// RateLimitResult
pub struct RateLimitResult {
    pub allowed:   bool,
    pub remaining: u32,
    pub reset_at:  SystemTime,
}

// IdempotencyStore (lines 148-157)
#[async_trait::async_trait]
pub trait IdempotencyStore: Send + Sync {
    async fn check_and_store(&self, key: &str, ttl: Duration) -> Result<bool, RuntimeError>;
    async fn get_result(&self, key: &str) -> Result<Option<serde_json::Value>, RuntimeError>;
    async fn store_result(&self, key: &str, result: &serde_json::Value) -> Result<(), RuntimeError>;
}
```

### Error Type

`RuntimeError` (from `fraiseql-error`) — use `RuntimeError::Internal` for mock
error simulation:
```rust
RuntimeError::Internal {
    message: String,
    source:  Option<Box<dyn std::error::Error + Send + Sync>>,
}
```

### Existing Patterns to Follow

- `InMemorySessionStore` (`auth/session.rs:140-238`) — `DashMap`, helper methods,
  `Default` impl
- `MockIdempotencyStore` (`webhooks/testing.rs`) — builder pattern, `Mutex<HashMap>`
- `MockClock` (`webhooks/testing.rs`) — `AtomicU64` for time manipulation

### Available Dependencies

`dashmap = "5.5"`, `async-trait = "0.1"`, `tokio = "1.45"`, `serde_json = "1.0"`

## TDD Cycles

### Cycle 1: MockCacheClient

**File:** `crates/fraiseql-server/src/testing/runtime_testing.rs`

- **RED**:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use std::time::Duration;

      #[tokio::test]
      async fn cache_get_miss() {
          let cache = MockCacheClient::new();
          let result = cache.get("nonexistent").await.unwrap();
          assert!(result.is_none());
      }

      #[tokio::test]
      async fn cache_set_and_get() {
          let cache = MockCacheClient::new();
          cache.set("key1", b"value1", None).await.unwrap();
          let result = cache.get("key1").await.unwrap();
          assert_eq!(result, Some(b"value1".to_vec()));
      }

      #[tokio::test]
      async fn cache_delete() {
          let cache = MockCacheClient::new();
          cache.set("key1", b"value1", None).await.unwrap();
          cache.delete("key1").await.unwrap();
          assert!(cache.get("key1").await.unwrap().is_none());
      }

      #[tokio::test]
      async fn cache_ttl_expiry() {
          let cache = MockCacheClient::new();
          cache.set("key1", b"value1", Some(Duration::from_millis(50))).await.unwrap();
          tokio::time::sleep(Duration::from_millis(100)).await;
          assert!(cache.get("key1").await.unwrap().is_none());
      }

      #[tokio::test]
      async fn cache_ping() {
          let cache = MockCacheClient::new();
          assert!(cache.ping().await.is_ok());
      }
  }
  ```

- **GREEN**:
  ```rust
  use std::time::{Duration, Instant, SystemTime};
  use async_trait::async_trait;
  use dashmap::DashMap;
  use fraiseql_error::RuntimeError;
  use crate::runtime_state::{CacheClient, RateLimiter, RateLimitResult, IdempotencyStore};

  pub struct MockCacheClient {
      store: DashMap<String, (Vec<u8>, Option<Instant>)>,
  }

  impl MockCacheClient {
      pub fn new() -> Self {
          Self { store: DashMap::new() }
      }
  }

  impl Default for MockCacheClient {
      fn default() -> Self { Self::new() }
  }

  #[async_trait]
  impl CacheClient for MockCacheClient {
      async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, RuntimeError> {
          match self.store.get(key) {
              Some(entry) => {
                  if let Some(expiry) = entry.1 {
                      if Instant::now() > expiry {
                          drop(entry);
                          self.store.remove(key);
                          return Ok(None);
                      }
                  }
                  Ok(Some(entry.0.clone()))
              }
              None => Ok(None),
          }
      }

      async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<(), RuntimeError> {
          let expiry = ttl.map(|d| Instant::now() + d);
          self.store.insert(key.to_string(), (value.to_vec(), expiry));
          Ok(())
      }

      async fn delete(&self, key: &str) -> Result<(), RuntimeError> {
          self.store.remove(key);
          Ok(())
      }

      async fn ping(&self) -> Result<(), RuntimeError> {
          Ok(())
      }
  }
  ```

- **REFACTOR**: Add inspection helpers (`len()`, `is_empty()`, `clear()`) for
  test assertions
- **CLEANUP**: Clippy, test, commit

---

### Cycle 2: MockRateLimiter

- **RED**:
  ```rust
  #[tokio::test]
  async fn rate_limiter_allows_under_limit() {
      let limiter = MockRateLimiter::new();
      let result = limiter.check("user:1", 5, Duration::from_secs(60)).await.unwrap();
      assert!(result.allowed);
      assert_eq!(result.remaining, 4);
  }

  #[tokio::test]
  async fn rate_limiter_blocks_over_limit() {
      let limiter = MockRateLimiter::new();
      for _ in 0..5 {
          limiter.check("user:1", 5, Duration::from_secs(60)).await.unwrap();
      }
      let result = limiter.check("user:1", 5, Duration::from_secs(60)).await.unwrap();
      assert!(!result.allowed);
      assert_eq!(result.remaining, 0);
  }

  #[tokio::test]
  async fn rate_limiter_always_allow() {
      let limiter = MockRateLimiter::always_allow();
      for _ in 0..100 {
          let result = limiter.check("user:1", 1, Duration::from_secs(1)).await.unwrap();
          assert!(result.allowed);
      }
  }
  ```

- **GREEN**:
  ```rust
  pub struct MockRateLimiter {
      counters: DashMap<String, (u32, Instant)>,  // count + window start
      always_allow: bool,
  }

  impl MockRateLimiter {
      pub fn new() -> Self {
          Self { counters: DashMap::new(), always_allow: false }
      }

      pub fn always_allow() -> Self {
          Self { counters: DashMap::new(), always_allow: true }
      }
  }

  impl Default for MockRateLimiter {
      fn default() -> Self { Self::new() }
  }

  #[async_trait]
  impl RateLimiter for MockRateLimiter {
      async fn check(&self, key: &str, limit: u32, window: Duration) -> Result<RateLimitResult, RuntimeError> {
          if self.always_allow {
              return Ok(RateLimitResult {
                  allowed: true,
                  remaining: limit.saturating_sub(1),
                  reset_at: SystemTime::now() + window,
              });
          }

          let now = Instant::now();
          let mut entry = self.counters.entry(key.to_string()).or_insert((0, now));

          // Reset window if expired
          if now.duration_since(entry.1) >= window {
              entry.0 = 0;
              entry.1 = now;
          }

          entry.0 += 1;
          let allowed = entry.0 <= limit;
          let remaining = if allowed { limit - entry.0 } else { 0 };

          Ok(RateLimitResult {
              allowed,
              remaining,
              reset_at: SystemTime::now() + window,
          })
      }
  }
  ```

- **REFACTOR**: Add `always_deny()` constructor for error-path testing
- **CLEANUP**: Clippy, test, commit

---

### Cycle 3: MockIdempotencyStore

- **RED**:
  ```rust
  #[tokio::test]
  async fn idempotency_new_key_returns_true() {
      let store = MockIdempotencyStore::new();
      let is_new = store.check_and_store("req-1", Duration::from_secs(60)).await.unwrap();
      assert!(is_new, "first check should return true (new key)");
  }

  #[tokio::test]
  async fn idempotency_duplicate_key_returns_false() {
      let store = MockIdempotencyStore::new();
      store.check_and_store("req-1", Duration::from_secs(60)).await.unwrap();
      let is_new = store.check_and_store("req-1", Duration::from_secs(60)).await.unwrap();
      assert!(!is_new, "second check should return false (duplicate)");
  }

  #[tokio::test]
  async fn idempotency_store_and_get_result() {
      let store = MockIdempotencyStore::new();
      store.check_and_store("req-1", Duration::from_secs(60)).await.unwrap();

      let result = serde_json::json!({"status": "ok", "id": 42});
      store.store_result("req-1", &result).await.unwrap();

      let retrieved = store.get_result("req-1").await.unwrap();
      assert_eq!(retrieved, Some(result));
  }

  #[tokio::test]
  async fn idempotency_get_result_missing_key() {
      let store = MockIdempotencyStore::new();
      let retrieved = store.get_result("nonexistent").await.unwrap();
      assert!(retrieved.is_none());
  }
  ```

- **GREEN**:
  ```rust
  pub struct MockIdempotencyStore {
      store: DashMap<String, Option<serde_json::Value>>,
  }

  impl MockIdempotencyStore {
      pub fn new() -> Self {
          Self { store: DashMap::new() }
      }
  }

  impl Default for MockIdempotencyStore {
      fn default() -> Self { Self::new() }
  }

  #[async_trait]
  impl IdempotencyStore for MockIdempotencyStore {
      async fn check_and_store(&self, key: &str, _ttl: Duration) -> Result<bool, RuntimeError> {
          if self.store.contains_key(key) {
              Ok(false)
          } else {
              self.store.insert(key.to_string(), None);
              Ok(true)
          }
      }

      async fn get_result(&self, key: &str) -> Result<Option<serde_json::Value>, RuntimeError> {
          Ok(self.store.get(key).and_then(|entry| entry.value().clone()))
      }

      async fn store_result(&self, key: &str, result: &serde_json::Value) -> Result<(), RuntimeError> {
          self.store.insert(key.to_string(), Some(result.clone()));
          Ok(())
      }
  }
  ```

- **REFACTOR**: Add inspection helpers (`contains_key()`, `len()`)
- **CLEANUP**: Clippy, test, commit

---

### Cycle 4: Update Module Documentation

- **RED**: N/A
- **GREEN**: Replace the TODO block in `runtime_testing.rs:1-10` with proper
  module documentation listing the three available mocks and brief usage examples
- **REFACTOR**: N/A
- **CLEANUP**: Clippy, commit

## Dependencies
- None

## Status
[ ] Not Started
