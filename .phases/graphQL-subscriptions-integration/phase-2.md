# Phase 1: PyO3 Core Bindings - Implementation Plan

**Phase**: 1
**Objective**: Expose Rust subscription engine to Python with minimal overhead via PyO3 bindings
**Estimated Time**: 2 weeks / 30 hours
**Files Created**: 1 new Rust file (~500 lines)
**Success Criteria**: PySubscriptionExecutor callable from Python, all unit tests passing, `cargo build --lib` succeeds
**Lead Engineer**: Junior Rust/Python FFI Developer

---

## Context

Phase 1 creates the PyO3 bindings that allow Python code to interact with the Rust subscription engine. This is the foundation for all Python integration.

**Key Design Decisions**:
- Use existing global tokio runtime (from `crate::db::runtime`)
- Sync Python calls with internal async Rust work via `block_on()`
- Return pre-serialized bytes for performance
- Follow existing FraiseQL PyO3 patterns (see `auth/py_bindings.rs`, `apq/py_bindings.rs`)

---

## Files to Create/Modify

### New Files
- `fraiseql_rs/src/subscriptions/py_bindings.rs` (NEW, ~500 lines) - All PyO3 bindings

### Modified Files
- `fraiseql_rs/src/lib.rs` (modify) - Add subscriptions module registration
- `fraiseql_rs/src/subscriptions/mod.rs` (NEW) - Module declaration (if not exists)

---

## Detailed Implementation Tasks

### Task 1.1: Subscription Payload Types (6 hours)

**Objective**: Define Python-callable classes for subscription data structures

**Steps**:
1. Create `fraiseql_rs/src/subscriptions/py_bindings.rs`
2. Implement `PySubscriptionPayload` class
3. Implement `PyGraphQLMessage` class
4. Add helper functions for Python ↔ Rust conversion

**Code to Write**:

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;

// PySubscriptionPayload - matches GraphQL subscription format
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
            variables: Python::with_gil(|py| PyDict::new_bound(py).unbind()),
            extensions: None,
        }
    }
}

// PyGraphQLMessage - for WebSocket messages
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
        let type_ = data.get_item("type")?.extract::<String>()?;
        let id = data.get_item("id").ok().and_then(|i| i.extract::<String>().ok());
        let payload = data.get_item("payload").ok().and_then(|p| {
            if p.is_none() { None } else { p.downcast::<PyDict>().ok().map(|d| d.unbind()) }
        });

        Ok(Self { type_, id, payload })
    }

    pub fn to_dict(&self) -> PyResult<Py<PyDict>> {
        Python::with_gil(|py| {
            let dict = PyDict::new_bound(py);
            dict.set_item("type", &self.type_)?;
            if let Some(ref id) = self.id {
                dict.set_item("id", id)?;
            }
            if let Some(ref payload) = self.payload {
                dict.set_item("payload", payload)?;
            }
            Ok(dict.unbind())
        })
    }
}
```

**Acceptance Criteria**:
- [ ] `PySubscriptionPayload` can be instantiated: `payload = PySubscriptionPayload("query { test }")`
- [ ] `PyGraphQLMessage.from_dict()` works with valid dict
- [ ] `PyGraphQLMessage.to_dict()` returns correct dict
- [ ] All field access works (get/set)
- [ ] Code compiles without warnings

### Task 1.2: Core Subscription Executor (8 hours)

**Objective**: Implement the main PyO3 class that wraps Rust SubscriptionExecutor

**Steps**:
1. Add `PySubscriptionExecutor` class to `py_bindings.rs`
2. Implement all required methods
3. Add helper functions for conversions
4. Use existing global runtime pattern

**Code to Write**:

```rust
#[pyclass]
pub struct PySubscriptionExecutor {
    executor: Arc<SubscriptionExecutor>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PySubscriptionExecutor {
    #[new]
    pub fn new() -> PyResult<Self> {
        // Get global runtime from crate::db::runtime::init_runtime()
        // Clone the Arc<Runtime>
        // Create new SubscriptionExecutor
        // Return Self
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
        // Create SubscriptionSecurityContext from user_id/tenant_id
        // Store in executor (fast O(1) DashMap operation)
        // Return Ok(()) or PyErr
    }

    pub fn publish_event(
        &self,
        event_type: String,
        channel: String,
        data: &Bound<PyDict>,
    ) -> PyResult<()> {
        // Convert PyDict to Arc<Event>
        // Use self.runtime.block_on(async { executor.publish_event(event).await })
        // Return Ok(()) or PyErr
    }

    pub fn next_event(
        &self,
        subscription_id: String,
    ) -> PyResult<Option<Vec<u8>>> {
        // Get next pre-serialized bytes from response queue
        // Return Some(bytes) or None
    }

    pub fn complete_subscription(&self, subscription_id: String) -> PyResult<()> {
        // Cleanup subscription from registry
        // Clear response queue
        // Return Ok(()) or PyErr
    }

    pub fn get_metrics(&self) -> PyResult<Py<PyDict>> {
        // Get SecurityMetrics from executor
        // Convert to Python dict
        // Return Py<PyDict>
    }
}
```

**Helper Functions to Implement**:

```rust
fn python_dict_to_json_map(dict: &Bound<PyDict>) -> PyResult<HashMap<String, serde_json::Value>> {
    // Convert PyDict to HashMap<String, Value>
    // Handle nested objects, arrays, primitives
}

fn python_dict_to_event(
    event_type: String,
    channel: String,
    data: &Bound<PyDict>,
) -> PyResult<Arc<Event>> {
    // Create Arc<Event> with converted data
}

fn json_to_python_dict(py: Python, json: &HashMap<String, Value>) -> PyResult<Py<PyDict>> {
    // Convert JSON map back to PyDict
}

fn python_metrics_dict(metrics: &SecurityMetrics) -> PyResult<Py<PyDict>> {
    // Convert SecurityMetrics struct to Python dict
}
```

**Acceptance Criteria**:
- [ ] `PySubscriptionExecutor()` instantiates successfully
- [ ] `register_subscription()` accepts all parameters and stores data
- [ ] `publish_event()` processes event without blocking Python GIL
- [ ] `next_event()` returns `bytes` or `None`
- [ ] `complete_subscription()` cleans up correctly
- [ ] `get_metrics()` returns dict with expected fields
- [ ] All methods callable from Python
- [ ] No blocking operations outside runtime

### Task 1.3: Event Bus Bridge (6 hours)

**Objective**: Expose EventBusConfig creation to Python

**Steps**:
1. Add `PyEventBusConfig` class to `py_bindings.rs`
2. Implement static methods for different backends
3. Add validation for URLs and connection strings

**Code to Write**:

```rust
#[pyclass]
pub struct PyEventBusConfig {
    pub bus_type: String,  // "memory", "redis", "postgresql"
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
        // Validate Redis URL format
        // Create EventBusConfig::Redis { url, consumer_group }
        // Return Self
    }

    #[staticmethod]
    pub fn postgresql(connection_string: String) -> PyResult<Self> {
        // Validate PostgreSQL connection string
        // Create EventBusConfig::PostgreSQL { connection_string }
        // Return Self
    }
}
```

**Acceptance Criteria**:
- [ ] `PyEventBusConfig.memory()` works
- [ ] `PyEventBusConfig.redis()` validates URLs
- [ ] `PyEventBusConfig.postgresql()` validates connection strings
- [ ] Invalid inputs raise appropriate PyErr
- [ ] All methods callable from Python

### Task 1.4: Module Registration (5 hours)

**Objective**: Register all classes with Python module

**Steps**:
1. Add subscriptions module to `fraiseql_rs/src/lib.rs`
2. Create `init_subscriptions()` function in `py_bindings.rs`
3. Register all classes with `PyModule`

**Code to Write in lib.rs**:

```rust
// Add to fraiseql_rs/src/lib.rs
pub mod subscriptions {
    pub mod py_bindings;
    // ... existing modules
}

// In the #[pyfunction] that creates the module:
#[pyfunction]
fn fraiseql_rs() -> PyResult<Py<PyModule>> {
    // ... existing code ...

    // Add subscriptions submodule
    let subscriptions_module = PyModule::new_bound(py, "subscriptions")?;
    py_bindings::init_subscriptions(&subscriptions_module)?;
    m.add_submodule(&subscriptions_module)?;

    Ok(m)
}
```

**Code to Write in py_bindings.rs**:

```rust
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySubscriptionPayload>()?;
    m.add_class::<PyGraphQLMessage>()?;
    m.add_class::<PySubscriptionExecutor>()?;
    m.add_class::<PyEventBusConfig>()?;
    Ok(())
}
```

**Acceptance Criteria**:
- [ ] `cargo build --lib` succeeds
- [ ] Can import: `from fraiseql import _fraiseql_rs`
- [ ] Can access: `_fraiseql_rs.subscriptions.PySubscriptionExecutor`
- [ ] Can instantiate all classes from Python

---

## Testing Requirements

### Unit Tests (tests/test_subscriptions_phase1.py)

**Required Tests**:

```python
import pytest
from fraiseql import _fraiseql_rs

def test_payload_creation():
    payload = _fraiseql_rs.subscriptions.PySubscriptionPayload("query { test }")
    assert payload.query == "query { test }"

def test_message_conversion():
    msg = _fraiseql_rs.subscriptions.PyGraphQLMessage()
    msg.type_ = "connection_ack"
    dict_result = msg.to_dict()
    assert dict_result["type"] == "connection_ack"

def test_executor_instantiation():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
    assert executor is not None

@pytest.mark.asyncio
async def test_register_and_publish():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

    # Register subscription
    executor.register_subscription(
        connection_id="conn1",
        subscription_id="sub1",
        query="subscription { test }",
        variables={},
        user_id="user1",
        tenant_id="tenant1"
    )

    # Publish event
    executor.publish_event("test", "test", {"id": "123"})

    # Check next_event returns bytes or None
    result = executor.next_event("sub1")
    assert result is None or isinstance(result, bytes)

def test_event_bus_config():
    config = _fraiseql_rs.subscriptions.PyEventBusConfig.memory()
    assert config.bus_type == "memory"

def test_metrics():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
    metrics = executor.get_metrics()
    assert isinstance(metrics, dict)
```

**Run Tests**:
```bash
pytest tests/test_subscriptions_phase1.py -v
```

---

## Verification Checklist

- [ ] All code compiles: `cargo build --lib`
- [ ] No clippy warnings: `cargo clippy`
- [ ] Python import works: `python3 -c "from fraiseql import _fraiseql_rs; print(_fraiseql_rs.subscriptions)"`
- [ ] All unit tests pass
- [ ] Memory usage reasonable (no leaks)
- [ ] Methods respond quickly (<1ms for sync operations)

---

## Success Criteria for Phase 1

When Phase 1 is complete, this Python code should work:

```python
from fraiseql import _fraiseql_rs

# Create executor
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

# Register subscription
executor.register_subscription(
    connection_id="conn1",
    subscription_id="sub1",
    query="subscription { users { id } }",
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

# Get response (pre-serialized bytes)
response_bytes = executor.next_event("sub1")
if response_bytes:
    import json
    print("Response:", json.loads(response_bytes))

# Get metrics
metrics = executor.get_metrics()
print("Metrics:", metrics)
```

---

## Blockers & Dependencies

**Prerequisites**:
- Existing SubscriptionExecutor struct exists
- EventBusConfig enum exists
- SecurityMetrics struct exists
- Global runtime available via `crate::db::runtime::init_runtime()`

**Help Needed**:
- If global runtime access pattern unclear, ask senior engineer
- If existing SubscriptionExecutor API differs, ask senior engineer
- Reference existing PyO3 bindings for patterns

---

## Time Estimate Breakdown

- Task 1.1: 6 hours (research patterns + implement types)
- Task 1.2: 8 hours (implement core executor + helpers)
- Task 1.3: 6 hours (implement event bus config)
- Task 1.4: 5 hours (module registration + testing)
- Testing & fixes: 5 hours (run tests, fix issues)

**Total: 30 hours**

---

## Next Phase Dependencies

Phase 1 creates the PyO3 bindings that Phase 2 will extend with event dispatching logic. Phase 1 must be complete and tested before Phase 2 begins.</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-1.md
