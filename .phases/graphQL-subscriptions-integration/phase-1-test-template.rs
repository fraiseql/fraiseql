# Phase 1: Start Here - PyO3 Core Bindings

**Phase**: 1
**Time**: 2 weeks / 30 hours
**Goal**: Make Rust subscription engine callable from Python
**First Task**: Create `fraiseql_rs/src/subscriptions/py_bindings.rs`

---

## 🎯 What You're Building

By the end of Phase 1, Python code like this will work:

```python
from fraiseql import _fraiseql_rs

# Create the Rust executor
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

# Register a subscription
executor.register_subscription(
    connection_id="conn1",
    subscription_id="sub1",
    query="subscription { users { id } }",
    variables={},
    user_id="user1",
    tenant_id="tenant1",
)

# Publish an event
executor.publish_event(
    event_type="userCreated",
    channel="users",
    data={"id": "123", "name": "Alice"},
)

# Get the response (pre-serialized bytes)
response_bytes = executor.next_event("sub1")
if response_bytes:
    import json
    response = json.loads(response_bytes)
    print("Got subscription response:", response)
```

---

## 📁 File to Create

**Location**: `fraiseql_rs/src/subscriptions/py_bindings.rs`
**Size**: ~500 lines
**Purpose**: PyO3 bindings to expose Rust functionality to Python

---

## 🛠️ Step-by-Step Implementation

### Step 1: File Setup (5 minutes)

1. Create the file: `fraiseql_rs/src/subscriptions/py_bindings.rs`
2. Add basic imports:

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use serde_json::Value;
use std::sync::Arc;

// Import from existing modules (these may need to be created/adapted)
use crate::subscriptions::executor::SubscriptionExecutor;
use crate::db::runtime::init_runtime;
```

### Step 2: PySubscriptionPayload Class (30 minutes)

This is the first class - GraphQL subscription data.

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

**Test it**:
```bash
cargo build --lib
python3 -c "
from fraiseql import _fraiseql_rs
payload = _fraiseql_rs.subscriptions.PySubscriptionPayload('query { test }')
print('Query:', payload.query)
print('✅ PySubscriptionPayload works!')
"
```

### Step 3: PyGraphQLMessage Class (30 minutes)

WebSocket protocol messages.

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

**Test it**:
```python
from fraiseql import _fraiseql_rs

# Test message creation
msg = _fraiseql_rs.subscriptions.PyGraphQLMessage()
msg.type_ = "connection_ack"
msg.id = "123"

# Test dict conversion
dict_result = msg.to_dict()
assert dict_result["type"] == "connection_ack"
assert dict_result["id"] == "123"
print("✅ PyGraphQLMessage works!")
```

### Step 4: PySubscriptionExecutor Class (4 hours)

The main interface - this is the most complex part.

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
        // Get global runtime (adapt this to your existing pattern)
        let runtime = init_runtime().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to init runtime: {}", e)
            )
        })?;

        // Create executor (you'll need to implement SubscriptionExecutor::new())
        let executor = Arc::new(SubscriptionExecutor::new());

        Ok(Self { executor, runtime })
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
        // Convert PyDict to HashMap (implement helper)
        let variables_map = python_dict_to_json_map(variables)?;

        // Register with executor
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
        // Get next response bytes
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

### Step 5: Helper Functions (2 hours)

Implement the conversion helpers:

```rust
fn python_dict_to_json_map(dict: &Bound<PyDict>) -> PyResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    for (key, value) in dict.iter() {
        let key_str = key.extract::<String>()?;
        let value_json = python_to_json_value(value)?;
        map.insert(key_str, value_json);
    }
    Ok(map)
}

fn python_dict_to_event(
    event_type: String,
    channel: String,
    data: &Bound<PyDict>,
) -> PyResult<Event> {
    let data_map = python_dict_to_json_map(data)?;
    Ok(Event {
        event_type,
        channel,
        data: data_map,
    })
}

fn python_to_json_value(obj: &PyObject) -> PyResult<Value> {
    Python::with_gil(|py| {
        if let Ok(s) = obj.extract::<String>(py) {
            Ok(Value::String(s))
        } else if let Ok(i) = obj.extract::<i64>(py) {
            Ok(Value::Number(i.into()))
        } else if let Ok(f) = obj.extract::<f64>(py) {
            Ok(Value::Number(serde_json::Number::from_f64(f).unwrap()))
        } else if let Ok(b) = obj.extract::<bool>(py) {
            Ok(Value::Bool(b))
        } else if let Ok(list) = obj.extract::<Vec<PyObject>>(py) {
            let mut arr = Vec::new();
            for item in list {
                arr.push(python_to_json_value(&item)?);
            }
            Ok(Value::Array(arr))
        } else if let Ok(dict) = obj.downcast_bound::<PyDict>(py) {
            python_dict_to_json_map(&dict).map(Value::Object)
        } else {
            Ok(Value::Null)
        }
    })
}

// Add other helpers as needed...
```

### Step 6: PyEventBusConfig Class (1 hour)

Configuration for event buses:

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

### Step 7: Module Registration (30 minutes)

Add to `fraiseql_rs/src/lib.rs`:

```rust
// Add to lib.rs
pub mod subscriptions {
    pub mod py_bindings;
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

// In py_bindings.rs
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySubscriptionPayload>()?;
    m.add_class::<PyGraphQLMessage>()?;
    m.add_class::<PySubscriptionExecutor>()?;
    m.add_class::<PyEventBusConfig>()?;
    Ok(())
}
```

---

## ✅ Verification Steps

### 1. Compilation Check
```bash
cargo build --lib
# Should succeed with no errors
```

### 2. Import Check
```python
from fraiseql import _fraiseql_rs
print(dir(_fraiseql_rs.subscriptions))
# Should show: ['PySubscriptionPayload', 'PyGraphQLMessage', 'PySubscriptionExecutor', 'PyEventBusConfig']
```

### 3. Basic Functionality Test
```python
from fraiseql import _fraiseql_rs

# Test instantiation
executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()
config = _fraiseql_rs.subscriptions.PyEventBusConfig.memory()
payload = _fraiseql_rs.subscriptions.PySubscriptionPayload("query { test }")

print("✅ All classes instantiate successfully!")
```

### 4. End-to-End Test
Run the complete workflow from the beginning of this document.

---

## 🆘 Help & Common Issues

### Issue: "init_runtime not found"
- Check existing runtime initialization pattern in `crate::db::runtime`
- Adapt the call to match your existing API

### Issue: "SubscriptionExecutor not found"
- You need to implement or adapt the `SubscriptionExecutor` struct
- Look at existing executor patterns in the codebase

### Issue: "Event not found"
- Define an `Event` struct or use existing event structure
- Make sure it has `event_type`, `channel`, `data` fields

### Issue: Compilation errors
- Check PyO3 version compatibility
- Ensure all imports are correct
- Use `cargo check` for faster iteration

### Issue: Python import fails
- Make sure module registration is correct
- Check that `init_subscriptions` is called
- Verify `cargo build --lib` succeeded

---

## 📋 Next Steps

Once Phase 1 is complete:
1. **Commit** with message: `feat: Phase 1 - PyO3 core bindings for GraphQL subscriptions`
2. **Run tests** to verify functionality
3. **Update status** to Phase 1 ✅ Complete
4. **Start Phase 2** - Event distribution engine

---

## 📖 Reference

- **Detailed Plan**: `phase-1.md`
- **Checklist**: `phase-1-checklist.md`
- **Example Code**: `phase-1-implementation-example.py`
- **Planning Docs**: `IMPLEMENTATION_QUICK_START.md`

**Good luck with Phase 1! You've got this!** 🚀</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-1-start-here.md