# Subscriptions Python Integration - Implementation Quick Start

**Date**: January 3, 2026
**Status**: Ready for Phase 1
**Estimated Time to Phase 1 Complete**: 2 weeks / 30 hours

---

## What to Do First

### ✅ Review the Plans (Done)
- **SUBSCRIPTIONS_INTEGRATION_FINAL_PLAN.md** - Complete 5-phase plan
- **PLAN_V3_CHANGES_SUMMARY.md** - What changed with HTTP abstraction
- **SUBSCRIPTIONS_INTEGRATION_PLAN_V3_HTTP_ABSTRACTION.md** - Detailed V3 design

### ⏭️ Start Phase 1 Implementation

**Goal**: Create `fraiseql_rs/src/subscriptions/py_bindings.rs` with PyO3 bindings

**Estimated**: 2 weeks / 30 hours

---

## Phase 1 Breakdown

### 1.1: Subscription Payload Types (6 hours)

**File to create**: `fraiseql_rs/src/subscriptions/py_bindings.rs`

**What to implement**:

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass]
pub struct PySubscriptionPayload {
    #[pyo3(get, set)]
    pub query: String,
    #[pyo3(get, set)]
    pub operation_name: Option<String>,
    #[pyo3(get, set)]
    pub variables: Py<PyDict>,
    #[pyo3(get, set)]
    pub extensions: Option<Py<PyDict>>,
}

#[pymethods]
impl PySubscriptionPayload {
    #[new]
    pub fn new(query: String) -> Self {
        Self {
            query,
            operation_name: None,
            variables: /* create empty dict */,
            extensions: None,
        }
    }
}

#[pyclass]
pub struct PyGraphQLMessage {
    #[pyo3(get)]
    pub type_: String,
    #[pyo3(get)]
    pub id: Option<String>,
    #[pyo3(get)]
    pub payload: Option<Py<PyDict>>,
}

#[pymethods]
impl PyGraphQLMessage {
    #[staticmethod]
    pub fn from_dict(data: &Bound<PyDict>) -> PyResult<Self> {
        // Parse dict to message
    }

    pub fn to_dict(&self) -> Py<PyDict> {
        // Convert to dict
    }
}
```

**Acceptance Criteria**:
- [ ] Code compiles without errors
- [ ] Can instantiate `PySubscriptionPayload` from Python
- [ ] Can instantiate `PyGraphQLMessage` from Python
- [ ] Field access works (get/set properties)

---

### 1.2: Core Subscription Executor (8 hours)

**File**: Add to `fraiseql_rs/src/subscriptions/py_bindings.rs`

**What to implement**:

```rust
#[pyclass]
pub struct PySubscriptionExecutor {
    executor: Arc<SubscriptionExecutor>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PySubscriptionExecutor {
    #[new]
    pub fn new() -> Self {
        // Use global runtime from crate::db::runtime::init_runtime()
        // Get runtime via Arc clone
    }

    pub fn register_subscription(
        &self,
        connection_id: String,
        subscription_id: String,
        query: String,
        operation_name: Option<String>,
        variables: &Bound<PyDict>,
        user_id: String,
        tenant_id: String,
    ) -> PyResult<()> {
        // Convert PyDict variables to HashMap<String, Value>
        // Create SubscriptionSecurityContext
        // Store in executor DashMap
        // Return Ok(()) or PyErr
    }

    pub fn publish_event(
        &self,
        event_type: String,
        channel: String,
        data: &Bound<PyDict>,
    ) -> PyResult<()> {
        // Convert PyDict to Arc<Event>
        // Use runtime.block_on() to publish
        // Return Ok(()) or PyErr
    }

    pub fn next_event(
        &self,
        subscription_id: String,
    ) -> PyResult<Option<Vec<u8>>> {
        // Get next pre-serialized bytes from queue
        // Return Some(bytes) or None
    }

    pub fn complete_subscription(&self, subscription_id: String) -> PyResult<()> {
        // Cleanup subscription
        // Return Ok(()) or PyErr
    }

    pub fn get_metrics(&self) -> Py<PyDict> {
        // Convert SecurityMetrics to Python dict
        // Return Py<PyDict>
    }
}
```

**Key Implementation Notes**:
- `register_subscription()` is O(1) - just stores in DashMap
- `publish_event()` uses `self.runtime.block_on()` for async work
- `next_event()` returns pre-serialized Vec<u8> (critical for performance)
- All conversions: PyDict ↔ Rust types use helper functions

**Acceptance Criteria**:
- [ ] `register_subscription()` callable from Python
- [ ] `publish_event()` callable from Python
- [ ] `next_event()` returns `Vec<u8>` or None
- [ ] `complete_subscription()` callable
- [ ] `get_metrics()` returns dict with metrics
- [ ] Unit tests for each method pass
- [ ] No blocking calls outside runtime

---

### 1.3: Event Bus Bridge (6 hours)

**File**: Add to `fraiseql_rs/src/subscriptions/py_bindings.rs`

**What to implement**:

```rust
#[pyclass]
pub struct PyEventBusConfig {
    pub bus_type: String,
    pub config: EventBusConfig,
}

#[pymethods]
impl PyEventBusConfig {
    #[staticmethod]
    pub fn memory() -> Self {
        Self {
            bus_type: "memory".to_string(),
            config: EventBusConfig::InMemory,
        }
    }

    #[staticmethod]
    pub fn redis(url: String, consumer_group: String) -> PyResult<Self> {
        // Validate URL
        // Create EventBusConfig::Redis(...)
        // Return Self
    }

    #[staticmethod]
    pub fn postgresql(connection_string: String) -> PyResult<Self> {
        // Validate connection string
        // Create EventBusConfig::PostgreSQL(...)
        // Return Self
    }
}
```

**Acceptance Criteria**:
- [ ] `PyEventBusConfig.memory()` works
- [ ] `PyEventBusConfig.redis()` validates URL
- [ ] `PyEventBusConfig.postgresql()` validates connection string
- [ ] Unit tests pass

---

### 1.4: Module Registration (5 hours)

**File**: Update `fraiseql_rs/src/lib.rs`

**What to do**:

1. Add module declaration (if not exists):
```rust
pub mod subscriptions {
    pub mod py_bindings;
    // existing: executor, event_filter, metrics, etc.
}
```

2. In `fraiseql_rs()` function that creates PyModule:
```rust
fn fraiseql_rs() -> PyResult<Py<PyModule>> {
    // ... existing code ...

    // Add subscriptions module
    let subscriptions_module = PyModule::new_bound(py, "subscriptions")?;
    py_bindings::init_subscriptions(&subscriptions_module)?;
    m.add_submodule(&subscriptions_module)?;

    Ok(m)
}

// New function in subscriptions::py_bindings
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySubscriptionPayload>()?;
    m.add_class::<PyGraphQLMessage>()?;
    m.add_class::<PySubscriptionExecutor>()?;
    m.add_class::<PyEventBusConfig>()?;
    Ok(())
}
```

3. Build and verify:
```bash
cargo build --lib
python3 -c "from fraiseql import _fraiseql_rs; print(dir(_fraiseql_rs.subscriptions))"
```

**Acceptance Criteria**:
- [ ] `cargo build --lib` succeeds with zero errors
- [ ] Can import: `from fraiseql import _fraiseql_rs`
- [ ] Can access: `_fraiseql_rs.subscriptions.PySubscriptionExecutor`
- [ ] Can instantiate: `executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()`

---

## Helper Functions Needed

### Python Dict ↔ Rust Conversions

Add to `fraiseql_rs/src/subscriptions/py_bindings.rs`:

```rust
fn python_dict_to_json_map(
    dict: &Bound<PyDict>,
) -> PyResult<HashMap<String, serde_json::Value>> {
    // Convert PyDict to HashMap<String, Value>
}

fn python_dict_to_event(
    event_type: String,
    channel: String,
    data: &Bound<PyDict>,
) -> PyResult<Arc<Event>> {
    // Create Arc<Event> from Python dict
}

fn event_to_python_dict(
    py: Python,
    event: &Arc<Event>,
) -> PyResult<Py<PyDict>> {
    // Convert Event to PyDict
}

fn json_to_python_dict(
    py: Python,
    json: &HashMap<String, Value>,
) -> PyResult<Py<PyDict>> {
    // Convert JSON map to PyDict
}

fn python_to_json_value(
    py: Python,
    obj: &PyObject,
) -> PyResult<serde_json::Value> {
    // Convert any Python object to JSON
}

fn python_metrics_dict(
    metrics: &SecurityMetrics,
) -> Py<PyDict> {
    // Convert SecurityMetrics to PyDict
}
```

**Tip**: Look at existing PyO3 bindings in:
- `fraiseql_rs/src/auth/py_bindings.rs` - How other modules do conversions
- `fraiseql_rs/src/apq/py_bindings.rs` - More conversion examples

---

## Testing Phase 1

**Create**: `tests/test_subscriptions_phase1.py`

```python
import pytest
from fraiseql import _fraiseql_rs

def test_payload_creation():
    payload = _fraiseql_rs.subscriptions.PySubscriptionPayload(
        query="subscription { test }"
    )
    assert payload.query == "subscription { test }"

def test_executor_instantiation():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
    assert executor is not None

@pytest.mark.asyncio
async def test_register_and_publish():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

    # Register
    executor.register_subscription(
        connection_id="conn1",
        subscription_id="sub1",
        query="subscription { test }",
        operation_name=None,
        variables={},
        user_id="user1",
        tenant_id="tenant1",
    )

    # Publish event
    executor.publish_event(
        event_type="test",
        channel="test",
        data={"id": "123"},
    )

    # Get event
    response = executor.next_event("sub1")
    assert response is None or isinstance(response, bytes)

def test_event_bus_config_memory():
    config = _fraiseql_rs.subscriptions.PyEventBusConfig.memory()
    assert config is not None

def test_event_bus_config_redis():
    config = _fraiseql_rs.subscriptions.PyEventBusConfig.redis(
        url="redis://localhost:6379",
        consumer_group="test",
    )
    assert config is not None
```

**Run tests**:
```bash
pytest tests/test_subscriptions_phase1.py -v
```

---

## Implementation Checklist

### Before Starting
- [ ] Review SUBSCRIPTIONS_INTEGRATION_FINAL_PLAN.md
- [ ] Review existing py_bindings.rs files (auth, apq)
- [ ] Understand FraiseQL's Rust architecture
- [ ] Check existing SubscriptionExecutor structure

### Phase 1.1 (6 hours)
- [ ] Create py_bindings.rs file
- [ ] Implement PySubscriptionPayload struct
- [ ] Implement PyGraphQLMessage struct
- [ ] Unit tests pass
- [ ] Compile without warnings

### Phase 1.2 (8 hours)
- [ ] Implement PySubscriptionExecutor struct
- [ ] Implement register_subscription() method
- [ ] Implement publish_event() method
- [ ] Implement next_event() method
- [ ] Implement complete_subscription() method
- [ ] Implement get_metrics() method
- [ ] Unit tests pass (8+ test cases)
- [ ] Integration test: register → publish → get_event

### Phase 1.3 (6 hours)
- [ ] Implement PyEventBusConfig struct
- [ ] Implement PyEventBusConfig::memory()
- [ ] Implement PyEventBusConfig::redis()
- [ ] Implement PyEventBusConfig::postgresql()
- [ ] Unit tests pass
- [ ] Error handling for invalid configs

### Phase 1.4 (5 hours)
- [ ] Add module declaration to lib.rs
- [ ] Implement init_subscriptions() function
- [ ] Register all classes with PyModule
- [ ] `cargo build --lib` succeeds
- [ ] Python import test passes
- [ ] All subclasses accessible from Python

### Verification (5 hours reserved)
- [ ] Full Phase 1 test suite passes
- [ ] `cargo clippy` shows zero warnings
- [ ] Type checking clean
- [ ] Code review of conversions
- [ ] Performance: methods respond in <1ms

---

## Success Criteria for Phase 1

When Phase 1 is complete, you should be able to:

```python
from fraiseql import _fraiseql_rs

# Create executor
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

# Register subscription
executor.register_subscription(
    connection_id="conn1",
    subscription_id="sub1",
    query="subscription { users { id } }",
    operation_name=None,
    variables={},
    user_id="user1",
    tenant_id="tenant1",
)

# Publish event
executor.publish_event(
    event_type="userCreated",
    channel="users",
    data={"id": "123", "name": "Alice"},
)

# Get response
response_bytes = executor.next_event("sub1")
if response_bytes:
    print(json.loads(response_bytes))  # Should contain subscription response

# Get metrics
metrics = executor.get_metrics()
print(metrics)
```

✅ Phase 1 complete when this works with all types correct and tests passing.

---

## Timeline

```
Week 1:
├─ 1.1: Payload types (6 hrs)
├─ 1.2: Executor core (8 hrs)
└─ Review & fixes (10 hrs)

Week 2:
├─ 1.3: Event bus config (6 hrs)
├─ 1.4: Module registration (5 hrs)
└─ Testing & verification (9 hrs)

────────────────────────────────
Total: 30 hours (2 weeks)
```

---

## Next Steps After Phase 1

Once Phase 1 is complete and tested:

1. **Phase 2 starts** - Async event distribution engine
   - Extend EventBus trait
   - Implement dispatch_event_to_subscriptions()
   - Response queue management
   - Python resolver invocation

2. **Phase 3 follows** - Python high-level API
   - HTTP abstraction layer
   - SubscriptionManager class
   - FastAPI integration
   - Starlette integration

3. **Phase 4 & 5** - Testing and documentation

---

## Quick Reference

**Key Files**:
- Implementation: `fraiseql_rs/src/subscriptions/py_bindings.rs`
- Tests: `tests/test_subscriptions_phase1.py`
- Module registration: `fraiseql_rs/src/lib.rs`
- Reference implementations: `fraiseql_rs/src/auth/py_bindings.rs`, `fraiseql_rs/src/apq/py_bindings.rs`

**Key Crates**:
- `pyo3` - Python FFI
- `tokio` - Async runtime (already initialized)
- `dashmap` - Concurrent hashmap
- `serde_json` - JSON conversion

**Build & Test**:
```bash
# Build
cargo build --lib

# Test Rust
cargo test subscriptions

# Test Python
pytest tests/test_subscriptions_phase1.py -v

# Check for warnings
cargo clippy
```

---

## Ready to Begin Phase 1

All architecture decisions are finalized. You have:
- ✅ Complete 5-phase plan (130 hours)
- ✅ HTTP abstraction layer designed
- ✅ All critical gaps addressed
- ✅ Performance targets confirmed
- ✅ Phase 1 broken into 4 clear tasks (6-8 hours each)
- ✅ Success criteria defined

**Start with**: Create `fraiseql_rs/src/subscriptions/py_bindings.rs` and implement 1.1 (Payload types)

**Estimated completion**: 2 weeks from now
