use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::{
    cache::{CacheBackend, CachedActionResult},
    config::ActionConfig,
    error::Result,
    event::{EntityEvent, EventKind},
    traits::{ActionExecutor, ActionResult},
};

// Simple mock executor for testing
#[derive(Clone)]
struct TestExecutor {
    call_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl TestExecutor {
    fn new() -> Self {
        Self {
            call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ActionExecutor for TestExecutor {
    async fn execute(&self, _event: &EntityEvent, _action: &ActionConfig) -> Result<ActionResult> {
        self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(ActionResult {
            action_type: "test".to_string(),
            success:     true,
            message:     "Test success".to_string(),
            duration_ms: 10.0,
            status_code: None,
        })
    }
}

// Simple in-memory cache for testing
#[derive(Clone)]
struct InMemoryCache {
    store:       Arc<dashmap::DashMap<String, CachedActionResult>>,
    ttl_seconds: Arc<std::sync::atomic::AtomicU64>,
}

impl InMemoryCache {
    fn new() -> Self {
        Self {
            store:       Arc::new(dashmap::DashMap::new()),
            ttl_seconds: Arc::new(std::sync::atomic::AtomicU64::new(60)),
        }
    }
}

#[async_trait::async_trait]
impl CacheBackend for InMemoryCache {
    async fn get(&self, cache_key: &str) -> Result<Option<CachedActionResult>> {
        Ok(self.store.get(cache_key).map(|entry| entry.value().clone()))
    }

    async fn set(&self, cache_key: &str, result: &CachedActionResult) -> Result<()> {
        self.store.insert(cache_key.to_string(), result.clone());
        Ok(())
    }

    fn ttl_seconds(&self) -> u64 {
        self.ttl_seconds.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn set_ttl_seconds(&mut self, seconds: u64) {
        self.ttl_seconds.store(seconds, std::sync::atomic::Ordering::Relaxed);
    }

    async fn invalidate(&self, cache_key: &str) -> Result<()> {
        self.store.remove(cache_key);
        Ok(())
    }

    async fn clear_all(&self) -> Result<()> {
        self.store.clear();
        Ok(())
    }
}

#[tokio::test]
async fn test_cache_hit_does_not_execute_action() {
    let executor = TestExecutor::new();
    let cache = InMemoryCache::new();

    // Pre-populate cache
    let event = EntityEvent::new(
        EventKind::Created,
        "Test".to_string(),
        Uuid::new_v4(),
        json!({"name": "test"}),
    );

    let action = ActionConfig::Email {
        to:               Some("test@example.com".to_string()),
        to_template:      None,
        subject:          Some("Test".to_string()),
        subject_template: None,
        body_template:    Some("Test body".to_string()),
        reply_to:         None,
    };

    let cache_key = CachedActionExecutor::<TestExecutor, InMemoryCache>::cache_key(&event, &action);
    let cached_result =
        CachedActionResult::new("cached".to_string(), true, "Cached result".to_string(), 1.0);

    cache.set(&cache_key, &cached_result).await.unwrap();

    // Create cached executor
    let cached_executor = CachedActionExecutor::new(executor.clone(), cache);

    // Execute - should return cached result without calling inner executor
    let result = cached_executor.execute(&event, &action).await.unwrap();

    assert_eq!(result.action_type, "cached");
    assert!(result.success);
    assert_eq!(executor.call_count(), 0); // Inner executor NOT called
}

#[test]
fn test_cache_key_generation() {
    let event = EntityEvent::new(
        EventKind::Created,
        "Test".to_string(),
        Uuid::new_v4(), // entity_id (not event.id)
        json!({}),
    );

    let action = ActionConfig::Webhook {
        url:                Some("https://example.com".to_string()),
        url_env:            None,
        headers:            std::collections::HashMap::new(),
        body_template:      Some("{}".to_string()),
        signing_secret_env: None,
    };

    let key = CachedActionExecutor::<TestExecutor, InMemoryCache>::cache_key(&event, &action);

    // Cache key format: action_result:{event_id}:{action_debug}
    // Verify key contains the actual event.id (auto-generated)
    let expected_event_id = event.id.to_string();
    assert!(
        key.contains(&expected_event_id),
        "Key should contain event ID {expected_event_id}"
    );
    assert!(key.contains("Webhook"), "Key should contain action type");
    assert!(key.starts_with("action_result:"), "Key should start with action_result:");
}

#[tokio::test]
async fn test_cache_miss_executes_and_caches() {
    let executor = TestExecutor::new();
    let cache = InMemoryCache::new();
    let cached_executor = CachedActionExecutor::new(executor.clone(), cache.clone());

    let event = EntityEvent::new(
        EventKind::Created,
        "Test".to_string(),
        Uuid::new_v4(),
        json!({"name": "test"}),
    );

    let action = ActionConfig::Email {
        to:               Some("test@example.com".to_string()),
        to_template:      None,
        subject:          Some("Test".to_string()),
        subject_template: None,
        body_template:    Some("Test body".to_string()),
        reply_to:         None,
    };

    // First execution - cache miss
    let result1 = cached_executor.execute(&event, &action).await.unwrap();
    assert!(result1.success);
    assert_eq!(executor.call_count(), 1); // Inner executor called

    // Second execution - cache hit
    let result2 = cached_executor.execute(&event, &action).await.unwrap();
    assert!(result2.success);
    assert_eq!(executor.call_count(), 1); // Inner executor NOT called again
}
