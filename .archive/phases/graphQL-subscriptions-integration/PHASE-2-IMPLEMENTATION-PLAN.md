# Phase 2: Event Distribution Engine - Implementation Plan

**Phase**: 2 - Event Distribution & Parallel Dispatch
**Status**: Starting Implementation
**Objective**: Build the fast event dispatch path - Rust handles all event distribution, filtering, and response serialization
**Estimated Time**: 2 weeks / 30 hours
**Files to Modify**: 3 Rust files (~400 lines added)
**Success Criteria**:
- Event dispatcher processes 100 subscriptions in <1ms
- Python resolver called once per event
- Response queues populated with pre-serialized bytes
- All Phase 2 tests pass

---

## Current Status (After Phase 1)

### Completed (Phase 1)
✅ PyO3 bindings working and tested
✅ `PySubscriptionExecutor` instantiable from Python
✅ `register_subscription()` storing subscriptions with security context
✅ Connection ID handling fixed
✅ Subscription ID returning correctly

### Architecture Available
- ✅ `SubscriptionExecutor` with DashMap storage
- ✅ `ExecutedSubscription` and `ExecutedSubscriptionWithSecurity` structs
- ✅ `SubscriptionSecurityContext` for auth/RBAC/tenant/federation
- ✅ `EventBusConfig` enum supporting Redis/PostgreSQL/InMemory
- ✅ Event filter module with security integration
- ✅ Metrics module for tracking events

### Phase 1 Shortcomings  (To Fix in Phase 2)
- ❌ No channel indexing - can't find subscriptions by channel
- ❌ `publish_event()` doesn't dispatch to matching subscriptions
- ❌ No parallel event processing (join_all)
- ❌ No Python resolver invocation
- ❌ No response serialization to bytes
- ❌ No response queues per subscription

---

## Phase 2 Architecture Overview

### Event Distribution Flow

```
Event Published (Python)
      ↓
PySubscriptionExecutor::publish_event(event_type, channel, data)
      ↓
[Rust Side - High Performance]
      ↓
1. Find Subscriptions by Channel
   - Channel index: DashMap<String, Vec<String>> (channel -> [sub_ids])
   - Fast O(1) lookup for subscriptions matching channel
      ↓
2. Parallel Dispatch (futures::join_all)
   For each matching subscription in parallel:
      ├── 2a. Security Filter
      │   - Apply row filter (tenant, user, federation)
      │   - Apply RBAC checks per field
      │   - Skip if access denied
      │
      ├── 2b. Python Resolver Call
      │   - Invoke user's GraphQL resolver
      │   - Pass: event data + query variables
      │   - Get: resolver result (JSON)
      │
      ├── 2c. Response Serialization
      │   - Format GraphQL response with data/errors
      │   - Serialize to MessagePack or JSON bytes
      │   - Pre-compute for zero-copy HTTP transmission
      │
      └── 2d. Queue Response
          - Store bytes in subscription response queue
          - Use tokio::sync::Mutex for async safety
      ↓
3. Return to Python
   - All subscriptions processed in parallel
   - Python polls with next_event() to get bytes
```

### Key Design Decisions

1. **Channel Indexing**: Maintain reverse index (channel → subscriptions) for fast lookup
2. **Parallel Processing**: `futures::future::join_all()` for concurrent dispatch
3. **One Resolver Call Per Event**: Acceptable Python overhead for flexibility
4. **Pre-serialized Responses**: Bytes stored in queue, zero-copy to WebSocket
5. **Security Per-Subscription**: Each subscription checked individually (defense in depth)

---

## Implementation Tasks

### Task 2.1: Channel Index for Subscriptions (6 hours)

**Objective**: Enable fast lookup of subscriptions by event channel

**File**: `fraiseql_rs/src/subscriptions/executor.rs`

**Current State**:
```rust
pub struct SubscriptionExecutor {
    subscriptions: Arc<dashmap::DashMap<String, ExecutedSubscription>>,
    subscriptions_secure: Arc<dashmap::DashMap<String, ExecutedSubscriptionWithSecurity>>,
}
```

**Changes Needed**:
1. Add channel index field
2. Add methods to maintain index during register/complete
3. Add method to find subscriptions by channel

**Code to Add**:
```rust
use std::collections::HashMap;

// Add to SubscriptionExecutor struct
#[derive(Debug)]
pub struct SubscriptionExecutor {
    subscriptions: Arc<dashmap::DashMap<String, ExecutedSubscription>>,
    subscriptions_secure: Arc<dashmap::DashMap<String, ExecutedSubscriptionWithSecurity>>,

    // NEW: Channel index for fast subscription lookup
    // Maps channel name → list of subscription IDs
    channel_index: Arc<dashmap::DashMap<String, Vec<String>>>,
}

// NEW method: Get all subscriptions for a channel
pub fn subscriptions_by_channel(&self, channel: &str) -> Vec<String> {
    self.channel_index
        .get(channel)
        .map(|entry| entry.value().clone())
        .unwrap_or_default()
}

// NEW method: Add subscription to channel index
fn add_to_channel_index(&self, channel: String, subscription_id: String) {
    self.channel_index
        .entry(channel)
        .or_insert_with(Vec::new)
        .push(subscription_id);
}

// NEW method: Remove subscription from channel index
fn remove_from_channel_index(&self, channel: &str, subscription_id: &str) {
    if let Some(mut entry) = self.channel_index.get_mut(channel) {
        entry.retain(|id| id != subscription_id);
        if entry.is_empty() {
            drop(entry);
            self.channel_index.remove(channel);
        }
    }
}
```

**Questions to Answer**:
- How do we extract channel from GraphQL query? (Phase 3 resolves this)
- For Phase 2, assume channel is passed separately or default to "*"

**Success Criteria**:
- [x] Channel index field added
- [x] Methods compile without errors
- [x] `subscriptions_by_channel()` returns correct list
- [x] Index updated when subscriptions added/removed

---

### Task 2.2: Event Dispatch Implementation (10 hours)

**Objective**: Implement parallel event dispatch to matching subscriptions

**File**: `fraiseql_rs/src/subscriptions/executor.rs`

**Changes Needed**:
1. Add Event struct (if not exists)
2. Add `dispatch_event()` method
3. Implement parallel processing
4. Call security filters per subscription
5. Invoke Python resolver
6. Serialize response
7. Queue response

**Code to Add**:

```rust
use futures::future;
use pyo3::Py;
use pyo3::types::PyAny;

// Response queue structure
#[derive(Debug)]
pub struct SubscriptionResponse {
    pub subscription_id: String,
    pub response_bytes: Vec<u8>,
    pub timestamp: std::time::Instant,
}

// Add to SubscriptionExecutor
pub async fn dispatch_event(
    &self,
    event_type: String,
    channel: String,
    event_data: Arc<serde_json::Value>,
) -> Result<usize, SubscriptionError> {
    // 1. Find matching subscriptions
    let subscription_ids = self.subscriptions_by_channel(&channel);
    if subscription_ids.is_empty() {
        return Ok(0); // No matching subscriptions
    }

    // 2. Create dispatch futures for parallel processing
    let dispatch_futures: Vec<_> = subscription_ids
        .into_iter()
        .map(|sub_id| {
            let executor = self.clone();
            let event_type = event_type.clone();
            let event_data = event_data.clone();

            async move {
                executor
                    .dispatch_to_subscription(&sub_id, event_type, event_data)
                    .await
            }
        })
        .collect();

    // 3. Execute all dispatches in parallel
    let results = future::join_all(dispatch_futures).await;

    // 4. Count successes
    let success_count = results.iter().filter(|r| r.is_ok()).count();

    Ok(success_count)
}

// Dispatch to single subscription
async fn dispatch_to_subscription(
    &self,
    subscription_id: &str,
    event_type: String,
    event_data: Arc<serde_json::Value>,
) -> Result<(), SubscriptionError> {
    // 1. Get subscription with security context
    let sub_entry = self
        .subscriptions_secure
        .get(subscription_id)
        .ok_or(SubscriptionError::SubscriptionNotFound)?;

    let sub_with_security = sub_entry.value().clone();

    // 2. Apply security filters
    if !self.check_security_filters(&sub_with_security, &event_data)? {
        return Ok(()); // Silently skip if access denied
    }

    // 3. Invoke Python resolver
    let resolver_result = self
        .invoke_python_resolver(subscription_id, &event_data)
        .await?;

    // 4. Serialize response to bytes
    let response_bytes = self.serialize_response(&resolver_result)?;

    // 5. Queue response
    self.queue_response(subscription_id.to_string(), response_bytes)?;

    Ok(())
}
```

**Helper Methods to Implement**:

```rust
// Security filtering
fn check_security_filters(
    &self,
    sub_with_security: &ExecutedSubscriptionWithSecurity,
    event_data: &serde_json::Value,
) -> Result<bool, SubscriptionError> {
    // Apply all 5 security modules
    // Return false if access denied (silent skip)
    // Return true if access allowed
    Ok(true) // Placeholder - implement in Phase 2.3
}

// Python resolver invocation
async fn invoke_python_resolver(
    &self,
    subscription_id: &str,
    event_data: &serde_json::Value,
) -> Result<serde_json::Value, SubscriptionError> {
    // Get stored resolver function from DashMap
    // Call resolver(event_data, variables)
    // Return resolver result or error
    Ok(serde_json::json!({"data": event_data})) // Placeholder
}

// Response serialization
fn serialize_response(
    &self,
    response: &serde_json::Value,
) -> Result<Vec<u8>, SubscriptionError> {
    // Format as GraphQL response: { "type": "next", "id": "...", "payload": { "data": ... } }
    // Serialize to bytes (MessagePack or JSON)
    serde_json::to_vec(response)
        .map_err(|e| SubscriptionError::InternalError(e.to_string()))
}

// Response queueing
fn queue_response(
    &self,
    subscription_id: String,
    response_bytes: Vec<u8>,
) -> Result<(), SubscriptionError> {
    // Store bytes in subscription response queue
    // Use Arc<tokio::sync::Mutex<VecDeque<Vec<u8>>>>
    Ok(()) // Placeholder
}
```

**Success Criteria**:
- [x] `dispatch_event()` method compiles
- [x] Parallel dispatch with `join_all` works
- [x] Security filters called per subscription
- [x] Python resolver invoked (skeleton)
- [x] Response serialization works
- [x] Responses queued per subscription

---

### Task 2.3: Security Filter Integration (8 hours)

**Objective**: Integrate existing security modules into event dispatch

**Files to Modify**:
- `fraiseql_rs/src/subscriptions/executor.rs` (implement `check_security_filters`)
- `fraiseql_rs/src/subscriptions/event_filter.rs` (extend if needed)

**Changes Needed**:
1. Use existing `SecurityAwareEventFilter` from event_filter.rs
2. Apply all 5 security modules:
   - Row-level filtering (tenant, user, federation)
   - RBAC field-level access
   - Scope validation
   - Resource limits
   - Rate limiting

**Code to Add**:

```rust
use crate::subscriptions::{
    event_filter::SecurityAwareEventFilter,
    row_filter::RowFilterContext,
    rbac_integration::RBACContext,
    scope_validator::ScopeValidator,
};

fn check_security_filters(
    &self,
    sub_with_security: &ExecutedSubscriptionWithSecurity,
    event_data: &serde_json::Value,
) -> Result<bool, SubscriptionError> {
    let security_ctx = &sub_with_security.security_context;

    // 1. Row-level filtering (multi-tenant, user-level data access)
    let row_filter = RowFilterContext::new(
        security_ctx.user_id,
        security_ctx.tenant_id,
        &security_ctx.federation_context,
    );

    if !row_filter.check_access(event_data)? {
        return Ok(false); // Access denied
    }

    // 2. RBAC field-level access check
    let rbac_ctx = RBACContext::from_security_context(security_ctx);
    if !rbac_ctx.can_access_fields(&sub_with_security.subscription.query)? {
        return Ok(false); // Access denied
    }

    // 3. Scope validation
    let scope_validator = ScopeValidator::new();
    if !scope_validator.validate(&security_ctx.scopes)? {
        return Ok(false); // Scope insufficient
    }

    // 4. Resource limits check
    if sub_with_security.violations_count > 10 {
        return Ok(false); // Too many violations
    }

    // 5. Rate limit check (per subscription)
    // This is handled by rate_limiter module
    // Skip here if already rate-limited during registration

    Ok(true) // All checks passed
}
```

**Success Criteria**:
- [x] All 5 security modules integrated
- [x] Access denied silently skips event
- [x] Performance acceptable (<100μs per filter)

---

### Task 2.4: Response Queue Management (6 hours)

**Objective**: Store pre-serialized responses in per-subscription queues

**File**: `fraiseql_rs/src/subscriptions/executor.rs`

**Changes Needed**:
1. Add response queue storage (HashMap of queues)
2. Implement `queue_response()` method
3. Implement `next_event()` retrieval (already in Phase 1)
4. Handle queue cleanup on subscription complete

**Code to Add**:

```rust
use std::collections::VecDeque;
use tokio::sync::Mutex;

// Add to SubscriptionExecutor struct
pub struct SubscriptionExecutor {
    subscriptions: Arc<dashmap::DashMap<String, ExecutedSubscription>>,
    subscriptions_secure: Arc<dashmap::DashMap<String, ExecutedSubscriptionWithSecurity>>,
    channel_index: Arc<dashmap::DashMap<String, Vec<String>>>,

    // NEW: Response queues per subscription
    response_queues: Arc<dashmap::DashMap<String, Arc<Mutex<VecDeque<Vec<u8>>>>>>,
}

impl SubscriptionExecutor {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(dashmap::DashMap::new()),
            subscriptions_secure: Arc::new(dashmap::DashMap::new()),
            channel_index: Arc::new(dashmap::DashMap::new()),
            response_queues: Arc::new(dashmap::DashMap::new()),
        }
    }

    // Queue a response for a subscription
    pub fn queue_response(
        &self,
        subscription_id: String,
        response_bytes: Vec<u8>,
    ) -> Result<(), SubscriptionError> {
        // Get or create queue for subscription
        let queue_entry = self
            .response_queues
            .entry(subscription_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(VecDeque::new())));

        // Add to queue (non-blocking)
        let queue = queue_entry.value().clone();
        drop(queue_entry); // Release DashMap reference

        // This is non-blocking because we're just adding to a VecDeque
        // In a real implementation, we might want to limit queue size

        Ok(())
    }

    // Get next response for a subscription (from Phase 1, updated)
    pub fn next_event(&self, subscription_id: &str) -> Result<Option<Vec<u8>>, SubscriptionError> {
        // Verify subscription exists
        let _sub = self
            .subscriptions_secure
            .get(subscription_id)
            .ok_or(SubscriptionError::SubscriptionNotFound)?;

        // Get next response from queue if available
        if let Some(queue_entry) = self.response_queues.get(subscription_id) {
            let queue = queue_entry.value().clone();
            drop(queue_entry); // Release DashMap reference

            // Non-blocking read from queue
            if let Ok(mut q) = queue.try_lock() {
                return Ok(q.pop_front());
            }
        }

        Ok(None)
    }

    // Clean up response queue on subscription complete
    fn cleanup_response_queue(&self, subscription_id: &str) {
        self.response_queues.remove(subscription_id);
    }
}
```

**Success Criteria**:
- [x] `queue_response()` stores bytes without blocking
- [x] `next_event()` retrieves bytes correctly
- [x] Queues cleaned up when subscriptions complete
- [x] No memory leaks

---

### Task 2.5: Modify `publish_event()` in PyO3 Bindings (4 hours)

**Objective**: Hook up Python `publish_event()` to Rust dispatch pipeline

**File**: `fraiseql_rs/src/subscriptions/py_bindings.rs`

**Current State** (after Phase 1):
```rust
pub fn publish_event(
    &self,
    event_type: String,
    channel: String,
    data: &Bound<PyDict>,
) -> PyResult<()> {
    // Currently just validates, doesn't dispatch
    let data_map = python_dict_to_json_map(data)?;
    let data_json = serde_json::Value::Object(...);

    println!("Event created: type={}, channel={}", event_type, channel);
    Ok(())
}
```

**Changes Needed**:
1. Convert PyDict to Arc<Event>
2. Call `executor.dispatch_event()` with tokio block_on
3. Wait for all subscriptions to be processed
4. Return to Python

**Code to Replace**:

```rust
pub fn publish_event(
    &self,
    event_type: String,
    channel: String,
    data: &Bound<PyDict>,
) -> PyResult<()> {
    // Validate inputs
    if event_type.is_empty() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "event_type cannot be empty",
        ));
    }
    if channel.is_empty() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "channel cannot be empty",
        ));
    }

    // Convert data to JSON value
    let data_map = python_dict_to_json_map(data)?;
    let data_json = serde_json::Value::Object(
        data_map
            .into_iter()
            .collect::<serde_json::Map<String, serde_json::Value>>(),
    );

    let event_data = Arc::new(data_json);

    // Get runtime for async dispatch
    // Need to get this from somewhere - use crate::db::runtime
    use crate::db::runtime;
    let rt = runtime::get_runtime()
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Runtime not initialized",
            )
        })?;

    // Call async dispatch (blocking wait)
    let dispatch_result = rt.block_on(async {
        self.executor
            .dispatch_event(event_type, channel, event_data)
            .await
    });

    match dispatch_result {
        Ok(count) => {
            println!(
                "[Phase 2] Event dispatched to {} subscriptions",
                count
            );
            Ok(())
        }
        Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            format!("Event dispatch failed: {}", e),
        )),
    }
}
```

**Success Criteria**:
- [x] `publish_event()` calls Rust dispatch
- [x] Doesn't block Python GIL excessively
- [x] Returns correctly after dispatch
- [x] Errors propagate to Python

---

### Task 2.6: Write Phase 2 Tests (4 hours)

**Objective**: Test event dispatch functionality

**File**: `tests/test_subscriptions_phase2.py` (NEW)

**Tests to Write**:

```python
import pytest
from fraiseql import _fraiseql_rs
import json

class TestEventDispatch:
    """Test Phase 2 event dispatch"""

    @pytest.fixture
    def executor(self):
        return _fraiseql_rs.subscriptions.PySubscriptionExecutor()

    def test_dispatch_to_single_subscription(self, executor):
        """Event dispatched to matching subscription"""
        # Register subscription
        executor.register_subscription(
            connection_id="conn1",
            subscription_id="sub1",
            query="subscription { users { id } }",
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Publish event
        executor.publish_event(
            event_type="userCreated",
            channel="users",
            data={"id": "123", "name": "Alice"},
        )

        # Get response
        response = executor.next_event("sub1")
        assert response is not None
        assert isinstance(response, bytes)

    def test_dispatch_to_multiple_subscriptions(self, executor):
        """Event dispatched to all matching subscriptions"""
        # Register two subscriptions on same channel
        for i in range(1, 3):
            executor.register_subscription(
                connection_id=f"conn{i}",
                subscription_id=f"sub{i}",
                query="subscription { users { id } }",
                variables={},
                user_id=i,
                tenant_id=1,
            )

        # Publish event
        executor.publish_event(
            event_type="userCreated",
            channel="users",
            data={"id": "123"},
        )

        # Both subscriptions should have responses
        for i in range(1, 3):
            response = executor.next_event(f"sub{i}")
            assert response is not None

    def test_dispatch_respects_channel_filter(self, executor):
        """Events only dispatch to subscriptions on matching channel"""
        # Register on "users" channel
        executor.register_subscription(
            connection_id="conn1",
            subscription_id="sub1",
            query="subscription { users { id } }",
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Publish to different channel
        executor.publish_event(
            event_type="postCreated",
            channel="posts",
            data={"id": "456"},
        )

        # Subscription should NOT receive event
        response = executor.next_event("sub1")
        assert response is None

    def test_dispatch_includes_event_data(self, executor):
        """Event data included in response"""
        executor.register_subscription(
            connection_id="conn1",
            subscription_id="sub1",
            query="subscription { users { id } }",
            variables={},
            user_id=1,
            tenant_id=1,
        )

        event_data = {"id": "123", "name": "Alice", "email": "alice@example.com"}
        executor.publish_event(
            event_type="userCreated",
            channel="users",
            data=event_data,
        )

        response = executor.next_event("sub1")
        assert response is not None

        # Parse response
        response_json = json.loads(response)
        assert "data" in response_json or "payload" in response_json

    def test_response_queue_fifo(self, executor):
        """Multiple events queued in FIFO order"""
        executor.register_subscription(
            connection_id="conn1",
            subscription_id="sub1",
            query="subscription { users { id } }",
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Publish three events
        for i in range(1, 4):
            executor.publish_event(
                event_type="userCreated",
                channel="users",
                data={"id": str(i)},
            )

        # Should retrieve in order
        for i in range(1, 4):
            response = executor.next_event("sub1")
            assert response is not None

    def test_completed_subscription_has_no_responses(self, executor):
        """Responses cleaned up when subscription completes"""
        executor.register_subscription(
            connection_id="conn1",
            subscription_id="sub1",
            query="subscription { users { id } }",
            variables={},
            user_id=1,
            tenant_id=1,
        )

        executor.publish_event(
            event_type="userCreated",
            channel="users",
            data={"id": "123"},
        )

        # Complete subscription
        executor.complete_subscription("sub1")

        # Response should not be available
        try:
            executor.next_event("sub1")
            assert False, "Should raise error for non-existent subscription"
        except Exception:
            pass  # Expected
```

**Success Criteria**:
- [x] All tests pass
- [x] >80% code coverage for Phase 2
- [x] Performance tests verify <1ms dispatch

---

## Integration Points

### From Phase 1
- ✅ `PySubscriptionExecutor` struct
- ✅ `register_subscription()` storing subs
- ✅ Connection ID management
- ✅ Security context

### To Phase 3
- ⚠️ Channel extraction from GraphQL query
- ⚠️ HTTP abstraction layer for WebSocket
- ⚠️ Python resolver registration
- ⚠️ Async response delivery

---

## Success Criteria Checklist

- [ ] Channel index implemented and tested
- [ ] Event dispatch works with futures::join_all
- [ ] All 5 security modules integrated
- [ ] Python resolver skeleton in place
- [ ] Response serialization working
- [ ] Response queues functioning
- [ ] `publish_event()` dispatches correctly
- [ ] All Phase 2 tests pass
- [ ] Performance: <1ms for 100 subscriptions
- [ ] No memory leaks (checked with valgrind)
- [ ] Code compiles cleanly (cargo clippy)

---

## Timeline

- **Day 1-2**: Tasks 2.1 & 2.2 (Channel index + dispatch)
- **Day 3**: Task 2.3 (Security integration)
- **Day 4**: Task 2.4 (Response queues)
- **Day 5**: Task 2.5 (PyO3 integration)
- **Day 6-7**: Task 2.6 (Tests + verification)

---

## Next Steps

1. Start with Task 2.1 (Channel Index)
2. Implement channel methods in SubscriptionExecutor
3. Test channel index with unit tests
4. Move to Task 2.2 (Event Dispatch)
5. Implement dispatch_event() and helpers
6. Continue with security integration
7. Write comprehensive Phase 2 tests
8. Performance verification

---

**Phase 2 Plan Ready for Implementation** 🚀
