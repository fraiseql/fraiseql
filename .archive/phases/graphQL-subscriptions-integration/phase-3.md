# Phase 2: Async Event Distribution Engine - Implementation Plan

**Phase**: 2
**Objective**: Build the fast event dispatch path - Rust handles all event distribution, filtering, and Python resolver invocation
**Estimated Time**: 2 weeks / 30 hours
**Files Modified**: 3 existing Rust files (~200 lines added)
**Success Criteria**: Event dispatcher processes 100 subscriptions in <1ms, Python resolver called once per event, response queues populated
**Lead Engineer**: Junior Async Rust Developer

---

## Context

Phase 2 extends the existing SubscriptionExecutor with parallel event distribution. All heavy lifting stays in Rust - event dispatch, security filtering, rate limiting, and response serialization.

**Key Design Decisions**:
- Parallel dispatch using `futures::future::join_all()`
- One Python resolver call per event (acceptable overhead)
- Pre-serialized responses to bytes (zero-copy to HTTP)
- Response queues per subscription (lock-free with tokio::sync::Mutex)

---

## Files to Create/Modify

### Modified Files
- `fraiseql_rs/src/subscriptions/executor.rs` (extend ~120 lines) - Add dispatch methods
- `fraiseql_rs/src/subscriptions/event_filter.rs` (extend ~50 lines) - Integration with existing security
- `fraiseql_rs/src/subscriptions/metrics.rs` (extend ~30 lines) - Add dispatch metrics

### New Files
- None (extending existing files)

---

## Detailed Implementation Tasks

### Task 2.1: Enhanced EventBus Architecture (10 hours)

**Objective**: Extend EventBus trait to integrate with subscription executor

**File**: `fraiseql_rs/src/subscriptions/event_bus.rs` (extend)

**Steps**:
1. Add `publish_with_executor` method to EventBus trait
2. Implement in InMemory, Redis, and PostgreSQL backends
3. Ensure atomic publish + dispatch operation

**Code to Write**:

```rust
// Add to EventBus trait
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError>;

    // NEW: Integrated publish + dispatch
    async fn publish_with_executor(
        &self,
        event: Arc<Event>,
        executor: Arc<SubscriptionExecutor>,
    ) -> Result<(), SubscriptionError> {
        // First publish to event bus
        self.publish(event.clone()).await?;

        // Then dispatch to subscriptions
        executor.dispatch_event_to_subscriptions(&event).await?;

        Ok(())
    }
}

// Implement in each backend
impl EventBus for InMemoryEventBus {
    async fn publish_with_executor(
        &self,
        event: Arc<Event>,
        executor: Arc<SubscriptionExecutor>,
    ) -> Result<(), SubscriptionError> {
        // InMemory publish logic...
        self.publish(event.clone()).await?;

        // Dispatch to subscriptions
        executor.dispatch_event_to_subscriptions(&event).await?;

        Ok(())
    }
}

// Similar implementations for RedisEventBus and PostgreSQLEventBus
```

**Acceptance Criteria**:
- [ ] `publish_with_executor` method compiles
- [ ] All three backends implement the method
- [ ] Publish + dispatch happens atomically
- [ ] Existing `publish` method unchanged

### Task 2.2: Subscription Event Dispatcher (12 hours)

**Objective**: Implement parallel event distribution with security filtering

**File**: `fraiseql_rs/src/subscriptions/executor.rs` (extend ~120 lines)

**Steps**:
1. Add `dispatch_event_to_subscriptions` method
2. Add `dispatch_event_to_single` method
3. Add `invoke_python_resolver` method
4. Add `encode_response_bytes` method
5. Integrate with existing security modules

#### Async Dispatch Flow

```
Event Received
      ↓
Find Matching Subscriptions
      ↓
Parallel Processing (join_all)
├── Subscription A ── Security Filter ── Python Resolver ── Serialize ── Queue Response
├── Subscription B ── Security Filter ── Python Resolver ── Serialize ── Queue Response
├── Subscription C ── Security Filter ── Python Resolver ── Serialize ── Queue Response
└── ...
      ↓
All Responses Queued
      ↓
WebSocket Polling Returns Bytes
```

**Key Points**:
- Parallel processing prevents blocking
- Security filtering happens per subscription
- Python resolver calls are blocking but isolated
- Responses pre-serialized for performance

**Code to Write**:

```rust
impl SubscriptionExecutor {
    // NEW: Main dispatch method
    pub async fn dispatch_event_to_subscriptions(
        &self,
        event: &Arc<Event>,
    ) -> Result<(), SubscriptionError> {
        // Find all subscriptions listening on this channel
        let subscriptions = self.subscriptions_by_channel(&event.channel).await?;

        // Process in parallel using join_all
        let mut futures = vec![];
        for (sub_id, sub) in subscriptions {
            let event_clone = event.clone();
            futures.push(async move {
                self.dispatch_event_to_single(sub_id, &event_clone).await
            });
        }

        // Wait for all dispatches to complete
        futures::future::join_all(futures).await;

        Ok(())
    }

    // NEW: Single subscription dispatch
    async fn dispatch_event_to_single(
        &self,
        subscription_id: &str,
        event: &Arc<Event>,
    ) -> Result<(), SubscriptionError> {
        // 1. Get subscription metadata
        let subscription = self.get_subscription(subscription_id)?;

        // 2. Apply SecurityAwareEventFilter (all 5 modules)
        let security_context = subscription.security_context.clone();
        let filter_result = self.event_filter.filter_event(
            event,
            &security_context,
            subscription.rate_limiter.clone(),
        ).await?;

        if !filter_result.allowed {
            // Increment blocked metrics
            return Ok(()); // Silently drop
        }

        // 3. Invoke Python resolver (ONE blocking call)
        let result = self.invoke_python_resolver(
            &subscription.resolver_fn,
            &subscription.variables,
            event,
        ).await?;

        // 4. Encode response to pre-serialized bytes
        let response_bytes = self.encode_response_bytes(
            subscription_id,
            &subscription.operation_name,
            &result,
        )?;

        // 5. Queue for WebSocket delivery
        self.queue_response(subscription_id, response_bytes).await?;

        Ok(())
    }

    // NEW: Python resolver invocation
    async fn invoke_python_resolver(
        &self,
        resolver_fn: &Py<PyAny>,
        variables: &HashMap<String, Value>,
        event: &Arc<Event>,
    ) -> Result<PyObject, SubscriptionError> {
        // Convert event and variables to Python objects
        Python::with_gil(|py| {
            let event_dict = event_to_python_dict(py, event)?;
            let vars_dict = json_to_python_dict(py, variables)?;

            // Call resolver: resolver(event, variables)
            let result = resolver_fn.call1(py, (event_dict, vars_dict))?;

            Ok(result)
        })
    }

    // NEW: Response serialization
    fn encode_response_bytes(
        &self,
        subscription_id: &str,
        operation_name: &Option<String>,
        result: &PyObject,
    ) -> Result<Vec<u8>, SubscriptionError> {
        Python::with_gil(|py| {
            // Convert result to JSON
            let json_value = python_to_json_value(py, result)?;

            // Create GraphQL response
            let response = serde_json::json!({
                "type": "next",
                "id": subscription_id,
                "payload": {
                    "data": json_value,
                    "errors": null
                }
            });

            // Serialize to bytes
            let bytes = serde_json::to_vec(&response)?;
            Ok(bytes)
        })
    }
}
```

**Integration with Existing Security**:
- Use existing `SecurityAwareEventFilter` from Phase 4
- Leverage existing `RateLimiter` per user
- Use existing metrics collection

**Acceptance Criteria**:
- [ ] `dispatch_event_to_subscriptions` compiles and runs
- [ ] Parallel processing with `join_all`
- [ ] Security filtering integrated
- [ ] Python resolver called correctly
- [ ] Response bytes queued properly
- [ ] No blocking outside Python calls

### Task 2.3: Response Queue Management (8 hours)

**Objective**: Add lock-free response queues per subscription

**File**: `fraiseql_rs/src/subscriptions/executor.rs` (extend ~50 lines)

**Steps**:
1. Add response queue fields to SubscriptionExecutor
2. Add `queue_response` and `next_response` methods
3. Add notification system for WebSocket polling

**Code to Write**:

```rust
pub struct SubscriptionExecutor {
    subscriptions: Arc<DashMap<String, SubscriptionData>>,
    // NEW: Response queues
    response_queues: Arc<DashMap<String, Arc<tokio::sync::Mutex<VecDeque<Vec<u8>>>>>>,
    // NEW: Notification channels
    response_notifiers: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<()>>>,
}

impl SubscriptionExecutor {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(DashMap::new()),
            response_queues: Arc::new(DashMap::new()),
            response_notifiers: Arc::new(DashMap::new()),
        }
    }

    // NEW: Queue response bytes
    pub async fn queue_response(
        &self,
        subscription_id: &str,
        response_bytes: Vec<u8>,
    ) -> Result<(), SubscriptionError> {
        // Get or create queue
        let queue = self.response_queues
            .entry(subscription_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(VecDeque::new())));

        // Lock and push
        {
            let mut queue_guard = queue.lock().await;
            queue_guard.push_back(response_bytes);
        }

        // Notify WebSocket (if listener exists)
        if let Some(notifier) = self.response_notifiers.get(subscription_id) {
            let _ = notifier.send(()); // Ignore send errors
        }

        Ok(())
    }

    // NEW: Get next response (called from Python)
    pub fn next_response(&self, subscription_id: &str) -> Option<Vec<u8>> {
        // Non-blocking get
        if let Some(queue) = self.response_queues.get(subscription_id) {
            // Try lock without blocking
            if let Ok(mut queue_guard) = queue.try_lock() {
                queue_guard.pop_front()
            } else {
                None // Queue busy, try again later
            }
        } else {
            None
        }
    }

    // NEW: Setup notification channel
    pub fn setup_notifier(
        &self,
        subscription_id: &str,
    ) -> tokio::sync::mpsc::UnboundedReceiver<()> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.response_notifiers.insert(subscription_id.to_string(), tx);
        rx
    }
}
```

**Acceptance Criteria**:
- [ ] Response queues created per subscription
- [ ] `queue_response` adds bytes without blocking
- [ ] `next_response` returns bytes or None
- [ ] Notification system works
- [ ] Lock-free for reads (writes use async locks)

---

## Testing Requirements

### Unit Tests (Add to tests/test_subscriptions_phase2.rs)

**Required Tests**:

```rust
#[tokio::test]
async fn test_dispatch_event_to_subscriptions() {
    let executor = SubscriptionExecutor::new();

    // Register test subscription
    executor.register_subscription(/* ... */).await?;

    // Create test event
    let event = Arc::new(Event {
        event_type: "test".to_string(),
        channel: "test".to_string(),
        data: HashMap::new(),
    });

    // Dispatch
    executor.dispatch_event_to_subscriptions(&event).await?;

    // Verify response queued
    let response = executor.next_response("sub1");
    assert!(response.is_some());
}

#[tokio::test]
async fn test_parallel_dispatch() {
    // Register 100 subscriptions
    // Dispatch one event
    // Verify all 100 get responses
    // Measure time <1ms
}

#[tokio::test]
async fn test_security_filtering_integration() {
    // Register subscription with security context
    // Dispatch event that should be filtered
    // Verify no response queued
}

#[tokio::test]
async fn test_python_resolver_invocation() {
    // Mock Python resolver
    // Dispatch event
    // Verify resolver called with correct args
    // Verify response serialized correctly
}
```

### Performance Tests

```rust
#[tokio::test]
async fn test_dispatch_performance() {
    // 100 subscriptions, 1 event
    // Measure dispatch time <1ms
    // Verify all responses queued
}
```

**Run Tests**:
```bash
cargo test subscriptions_phase2
```

---

## Verification Checklist

- [ ] All code compiles: `cargo build --lib`
- [ ] No clippy warnings: `cargo clippy`
- [ ] Unit tests pass
- [ ] Performance: 100 subscriptions in <1ms
- [ ] Security filtering works
- [ ] Python resolver called once per event
- [ ] Response bytes correctly formatted
- [ ] Queues work without deadlocks

---

## Success Criteria for Phase 2

When Phase 2 is complete:

```rust
// Create executor with subscriptions registered
let executor = SubscriptionExecutor::new();

// Dispatch event
let event = Arc::new(Event { /* ... */ });
executor.dispatch_event_to_subscriptions(&event).await?;

// Verify responses queued for all matching subscriptions
for sub_id in matching_subscription_ids {
    let response = executor.next_response(sub_id);
    assert!(response.is_some());
    let response_json: Value = serde_json::from_slice(&response.unwrap())?;
    assert_eq!(response_json["type"], "next");
}
```

---

## Blockers & Dependencies

**Prerequisites**:
- Phase 1 PyO3 bindings complete
- Existing SecurityAwareEventFilter (from Phase 4)
- Existing RateLimiter
- Existing metrics system

**Help Needed**:
- If SecurityAwareEventFilter API unclear, ask senior engineer
- If Python FFI patterns unclear, reference Phase 1
- If performance issues, ask senior engineer

---

## Time Estimate Breakdown

- Task 2.1: 10 hours (EventBus trait extension + implementations)
- Task 2.2: 12 hours (Core dispatcher + Python integration)
- Task 2.3: 8 hours (Response queues + notifications)
- Testing & fixes: 0 hours (covered in estimate)

**Total: 30 hours**

---

## Next Phase Dependencies

Phase 2 creates the event dispatch engine that Phase 3 will expose through the Python HTTP abstraction layer. Phase 2 must be complete and performance-tested before Phase 3 begins.</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-2.md
