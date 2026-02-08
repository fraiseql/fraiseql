# Phase 1 Implementation Guide - Junior Engineer

**Phase**: 1 - PyO3 Core Bindings
**Difficulty**: Medium (First PyO3 experience)
**Time**: 2 weeks / 30 hours
**Mentor**: Senior Rust/Python FFI Developer

---

## 🎯 Your Mission

Create the PyO3 bindings that allow Python code to call the Rust subscription engine. By the end, Python developers can:

```python
from fraiseql import _fraiseql_rs

# Create Rust executor from Python
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

# Register subscriptions
executor.register_subscription(
    connection_id="conn1",
    subscription_id="sub1",
    query="subscription { users { id } }",
    variables={},
    user_id="user1",
    tenant_id="tenant1"
)

# Publish events
executor.publish_event("userCreated", "users", {"id": "123"})

# Get responses
response_bytes = executor.next_event("sub1")
```

---

## 📋 Prerequisites

### Knowledge Required
- [ ] Basic Rust (structs, impl, error handling)
- [ ] Basic Python (classes, dicts, exceptions)
- [ ] Understanding of FFI (foreign function interface)

### Environment Setup
- [ ] Rust toolchain installed (`rustc --version`)
- [ ] Python 3.8+ installed
- [ ] PyO3 installed (`cargo add pyo3`)
- [ ] Existing FraiseQL code accessible
- [ ] `cargo build --lib` works for existing code

### Files to Reference
- [ ] `fraiseql_rs/src/auth/py_bindings.rs` (existing PyO3 example)
- [ ] `fraiseql_rs/src/apq/py_bindings.rs` (another PyO3 example)
- [ ] `fraiseql_rs/src/lib.rs` (module registration pattern)

---

## 🛠️ Step-by-Step Implementation

### Step 1: Create the File (10 minutes)

1. Create `fraiseql_rs/src/subscriptions/py_bindings.rs`
2. Add basic structure:

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use serde_json::Value;

// TODO: Add imports as you implement
// use crate::subscriptions::executor::SubscriptionExecutor;
// use crate::db::runtime::init_runtime;
```

2. Add to `fraiseql_rs/src/lib.rs`:

```rust
pub mod subscriptions {
    pub mod py_bindings;
}
```

3. Test: `cargo build --lib` should succeed

---

### Step 2: Implement PySubscriptionPayload (45 minutes)

**Goal**: Simple data class for GraphQL subscription info

```rust
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
```

**Test it:**
```python
from fraiseql import _fraiseql_rs
payload = _fraiseql_rs.subscriptions.PySubscriptionPayload("query { test }")
print(payload.query)  # Should print: query { test }
```

**Common Issues:**
- `#[pyo3(get, set)]` generates Python properties
- `Py<PyDict>` is a Python object reference
- `Python::with_gil()` required for Python object creation

---

### Step 3: Implement PyGraphQLMessage (45 minutes)

**Goal**: Data class for WebSocket messages

```rust
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

**Key Concepts:**
- `Bound<PyDict>` is a reference to a Python dict
- `extract::<String>()` converts Python str to Rust String
- `?` propagates errors as PyErr

**Test Commands:**
```python
# Test creation
msg = _fraiseql_rs.subscriptions.PyGraphQLMessage()
msg.type_ = "connection_ack"

# Test dict conversion
data = {"type": "next", "id": "sub1"}
msg = _fraiseql_rs.subscriptions.PyGraphQLMessage.from_dict(data)
result = msg.to_dict()
```

---

### Step 4: Implement PyEventBusConfig (30 minutes)

**Goal**: Configuration for event bus backends

```rust
#[pyclass]
pub struct PyEventBusConfig {
    pub bus_type: String,
    pub config: EventBusConfig,  // You'll need to define EventBusConfig
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
        if !url.starts_with("redis://") {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid Redis URL"));
        }
        Ok(Self {
            bus_type: "redis".to_string(),
            config: EventBusConfig::Redis { url, consumer_group },
        })
    }

    #[staticmethod]
    pub fn postgresql(connection_string: String) -> PyResult<Self> {
        if !connection_string.contains("postgresql://") {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid PostgreSQL connection string"));
        }
        Ok(Self {
            bus_type: "postgresql".to_string(),
            config: EventBusConfig::PostgreSQL { connection_string },
        })
    }
}
```

**Note:** You'll need to define the `EventBusConfig` enum. For Phase 1, you can create a simple version:

```rust
#[derive(Clone)]
pub enum EventBusConfig {
    InMemory,
    Redis { url: String, consumer_group: String },
    PostgreSQL { connection_string: String },
}
```

---

### Step 5: Implement PySubscriptionExecutor (8 hours - Most Complex)

**Goal**: Main interface to Rust subscription engine

This is the most complex part. Let's break it down:

#### Part 1: Basic Structure
```rust
#[pyclass]
pub struct PySubscriptionExecutor {
    executor: Arc<SubscriptionExecutor>,  // You'll need to define this
    runtime: Arc<tokio::runtime::Runtime>,
}
```

#### Part 2: Constructor
```rust
#[pymethods]
impl PySubscriptionExecutor {
    #[new]
    pub fn new() -> PyResult<Self> {
        // Get the global runtime (adapt to your existing pattern)
        let runtime = init_runtime().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to init runtime: {}", e)
            )
        })?;

        // Create executor (you'll implement this)
        let executor = Arc::new(SubscriptionExecutor::new());

        Ok(Self { executor, runtime })
    }
```

#### Part 3: Core Methods
```rust
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
        // Convert PyDict to HashMap (implement helper)
        let variables_map = python_dict_to_json_map(variables)?;

        // Call executor
        self.executor.register_subscription(
            connection_id,
            subscription_id,
            query,
            operation_name,
            variables_map,
            user_id,
            tenant_id,
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    pub fn publish_event(
        &self,
        event_type: String,
        channel: String,
        data: &Bound<PyDict>,
    ) -> PyResult<()> {
        // Convert to Event (implement helper)
        let event = python_dict_to_event(event_type, channel, data)?;

        // Use runtime for async operation
        self.runtime.block_on(async {
            self.executor.publish_event(event).await
        }).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    pub fn next_event(&self, subscription_id: String) -> PyResult<Option<Vec<u8>>> {
        Ok(self.executor.next_response(&subscription_id))
    }

    pub fn complete_subscription(&self, subscription_id: String) -> PyResult<()> {
        self.executor.complete_subscription(&subscription_id)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    pub fn get_metrics(&self) -> PyResult<Py<PyDict>> {
        let metrics = self.executor.get_metrics();
        python_metrics_dict(metrics)
    }
}
```

#### Part 4: Helper Functions (Implement these)

You'll need to implement conversion helpers. Here are the key ones:

```rust
fn python_dict_to_json_map(dict: &Bound<PyDict>) -> PyResult<HashMap<String, Value>> {
    // Convert PyDict to HashMap<String, Value>
    // Handle strings, numbers, booleans, arrays, objects
}

fn python_dict_to_event(
    event_type: String,
    channel: String,
    data: &Bound<PyDict>,
) -> PyResult<Event> {
    // Convert to your Event struct
}

fn python_to_json_value(obj: &PyObject) -> PyResult<Value> {
    // Convert Python object to serde_json::Value
    // Handle all JSON types
}

fn json_to_python_dict(py: Python, json: &HashMap<String, Value>) -> PyResult<Py<PyDict>> {
    // Convert back to Python dict
}

fn python_metrics_dict(metrics: &SecurityMetrics) -> PyResult<Py<PyDict>> {
    // Convert SecurityMetrics to Python dict
    // Implementation depends on SecurityMetrics struct
    Python::with_gil(|py| {
        let dict = PyDict::new_bound(py);
        // Add metrics fields...
        Ok(dict.unbind())
    })
}
```

## Automated Checklist Completion

### Checklist Status Script

Create `scripts/checklist-status.py`:

```python
#!/usr/bin/env python3
"""
Automated checklist status checker
Usage: python scripts/checklist-status.py
"""

import os
import re
from pathlib import Path

def check_file_for_checkboxes(filepath):
    """Check markdown file for checkbox completion."""
    try:
        with open(filepath, 'r') as f:
            content = f.read()

        # Find all checkboxes
        total_checkboxes = len(re.findall(r'- \[ \]', content))
        completed_checkboxes = len(re.findall(r'- \[x\]', content))

        return {
            'total': total_checkboxes,
            'completed': completed_checkboxes,
            'completion_rate': completed_checkboxes / total_checkboxes if total_checkboxes > 0 else 0
        }
    except FileNotFoundError:
        return {'total': 0, 'completed': 0, 'completion_rate': 0}

def main():
    """Check all phase checklists."""
    checklist_dir = Path('.phases/graphQL-subscriptions-integration')

    checklists = [
        'phase-1-checklist.md',
        'phase-2-checklist.md',
        'phase-3-checklist.md',
        'phase-4-checklist.md',
        'phase-5-checklist.md'
    ]

    print("Phase Checklist Completion Status")
    print("=" * 40)

    for checklist in checklists:
        filepath = checklist_dir / checklist
        status = check_file_for_checkboxes(filepath)

        phase_name = checklist.replace('-checklist.md', '').replace('phase-', 'Phase ')
        completion_pct = status['completion_rate'] * 100

        status_icon = "✅" if completion_pct == 100 else "🔄" if completion_pct > 0 else "⏳"

        print("12")

if __name__ == "__main__":
    main()
```

**Output Example**:
```
Phase Checklist Completion Status
========================================
Phase 1: ✅ 100% (24/24 completed)
Phase 2: 🔄 65% (15/23 completed)
Phase 3: ⏳ 0% (0/28 completed)
Phase 4: ⏳ 0% (0/32 completed)
Phase 5: ⏳ 0% (0/18 completed)
========================================
Overall: 23% complete
```

### Script Usage
```bash
# Run status check
python scripts/checklist-status.py

# Add to CI/CD
# This can be automated in deployment pipelines
```
```

**Key Challenges:**
- Understanding `Bound<PyDict>` vs `Py<PyDict>`
- Using `Python::with_gil()` for Python operations
- Error handling with `PyResult` and `?`
- Converting between Python and Rust types
- Understanding async runtime usage

---

### Step 6: Module Registration (30 minutes)

**Goal**: Make classes available to Python

Add to `fraiseql_rs/src/lib.rs`:

```rust
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

Add to `py_bindings.rs`:

```rust
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySubscriptionPayload>()?;
    m.add_class::<PyGraphQLMessage>()?;
    m.add_class::<PySubscriptionExecutor>()?;
    m.add_class::<PyEventBusConfig>()?;
    Ok(())
}
```

**Test:**
```python
from fraiseql import _fraiseql_rs
print(dir(_fraiseql_rs.subscriptions))
# Should show your classes
```

---

### Step 7: Stub Required Types (2 hours)

You'll need to create some stub types for Phase 1. These will be properly implemented in later phases:

```rust
// Stub Event struct
#[derive(Clone)]
pub struct Event {
    pub event_type: String,
    pub channel: String,
    pub data: HashMap<String, Value>,
}

// Stub SubscriptionExecutor
pub struct SubscriptionExecutor;

impl SubscriptionExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn register_subscription(
        &self,
        _connection_id: String,
        _subscription_id: String,
        _query: String,
        _operation_name: Option<String>,
        _variables: HashMap<String, Value>,
        _user_id: String,
        _tenant_id: String,
    ) -> Result<(), String> {
        // Stub implementation
        Ok(())
    }

    pub async fn publish_event(&self, _event: Event) -> Result<(), String> {
        // Stub implementation
        Ok(())
    }

    pub fn next_response(&self, _subscription_id: &str) -> Option<Vec<u8>> {
        // Stub implementation - return None for Phase 1
        None
    }

    pub fn complete_subscription(&self, _subscription_id: &str) -> Result<(), String> {
        // Stub implementation
        Ok(())
    }

    pub fn get_metrics(&self) -> SecurityMetrics {
        // Stub implementation
        SecurityMetrics {
            active_subscriptions: 0,
            total_events_processed: 0,
        }
    }
}

// Stub SecurityMetrics
#[derive(Clone)]
pub struct SecurityMetrics {
    pub active_subscriptions: u64,
    pub total_events_processed: u64,
}
```

---

## 🧪 Testing Your Implementation

### Unit Tests
Use the test template from `phase-1-test-template.py`. Key tests:

```python
def test_executor_instantiation():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
    assert executor is not None

def test_register_subscription():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
    executor.register_subscription(
        connection_id="conn1",
        subscription_id="sub1",
        query="subscription { test }",
        variables={},
        user_id="user1",
        tenant_id="tenant1",
    )
    # Should not raise exception

def test_publish_event():
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
    executor.publish_event("test", "test", {"data": "test"})
    # Should not raise exception
```

### Integration Test
```python
from fraiseql import _fraiseql_rs

# Complete workflow test
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

executor.register_subscription(
    connection_id="test_conn",
    subscription_id="test_sub",
    query="subscription { users { id } }",
    variables={},
    user_id="test_user",
    tenant_id="test_tenant",
)

executor.publish_event(
    event_type="userCreated",
    channel="users",
    data={"id": "123", "name": "Alice"},
)

response = executor.next_event("test_sub")
metrics = executor.get_metrics()

print("✅ Phase 1 implementation working!")
```

---

## 🆘 Common Issues & Solutions

### Issue: "pyo3 not found"
```bash
cargo add pyo3
```

### Issue: "serde_json not found"
```bash
cargo add serde_json
```

### Issue: "init_runtime not found"
- Find the existing runtime initialization in `crate::db::runtime`
- Adapt the call to match your codebase

### Issue: Compilation errors with `Bound<PyDict>`
- Make sure you're using PyO3 0.20+
- Check the PyO3 migration guide for API changes

### Issue: "GIL error" or "Python not initialized"
- Always use `Python::with_gil(|py| { ... })` for Python operations
- Don't call Python APIs without GIL

### Issue: "Type conversion failed"
- Check your `python_to_json_value` function
- Handle all JSON types: string, number, boolean, array, object, null

### Issue: Runtime blocking
- `runtime.block_on()` should release the GIL
- If it blocks, check your async function for blocking operations

### Issue: Python import fails
- Verify `cargo build --lib` succeeded
- Check module registration in `lib.rs`
- Ensure `init_subscriptions` is called

---

## 📚 Learning Resources

### PyO3 Documentation
- [PyO3 User Guide](https://pyo3.rs/v0.20.0/)
- [PyO3 Classes](https://pyo3.rs/v0.20.0/class.html)
- [PyO3 Error Handling](https://pyo3.rs/v0.20.0/exception.html)

### FraiseQL Examples
- `fraiseql_rs/src/auth/py_bindings.rs` - Complete working example
- `fraiseql_rs/src/apq/py_bindings.rs` - Another working example

### Rust Concepts
- [Ownership and Borrowing](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
- [Error Handling](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Async/Await](https://rust-lang.github.io/async-book/01_getting_started/01_chapter.html)

---

## ✅ Phase 1 Success Checklist

- [ ] `cargo build --lib` succeeds
- [ ] Python can import `_fraiseql_rs.subscriptions`
- [ ] All 4 classes are available
- [ ] `PySubscriptionExecutor()` instantiates
- [ ] `register_subscription()` works
- [ ] `publish_event()` works (even if no response yet)
- [ ] `next_event()` returns None (expected for Phase 1)
- [ ] `get_metrics()` returns dict
- [ ] Unit tests pass
- [ ] End-to-end test works

---

## 🎉 Completion

Once all tests pass:

1. **Commit** with message: `feat: Phase 1 - PyO3 core bindings for GraphQL subscriptions`
2. **Update status** to Phase 1 ✅ Complete
3. **Celebrate!** You've just created the foundation for the fastest GraphQL subscription system! 🚀
4. **Start Phase 2** - Event distribution engine

---

## 💬 Need Help?

- **Mentor**: Ask your senior Rust/Python FFI developer
- **Documentation**: Check `phase-1.md` for detailed requirements
- **Examples**: Look at existing PyO3 code in the codebase
- **Testing**: Use `phase-1-test-template.py` for guidance

**Remember**: Take it step by step. Each class builds on the previous one. You've got this! 💪</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/_phase-1-implementation-guide.md
