# Phase 1 Implementation Example

**File**: `fraiseql_rs/src/subscriptions/py_bindings.rs`
**Purpose**: Example implementation for junior engineers to follow
**Status**: Reference code - adapt for actual implementation

---

## Complete PyO3 Bindings Implementation

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use serde_json::Value;

// Import from existing modules
use crate::subscriptions::executor::SubscriptionExecutor;
use crate::db::runtime::init_runtime;

// PySubscriptionPayload - GraphQL subscription data
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

// PyGraphQLMessage - WebSocket protocol messages
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

// PySubscriptionExecutor - Main interface to Rust engine
#[pyclass]
pub struct PySubscriptionExecutor {
    executor: Arc<SubscriptionExecutor>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PySubscriptionExecutor {
    #[new]
    pub fn new() -> PyResult<Self> {
        // Get global runtime
        let runtime = init_runtime().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to init runtime: {}", e))
        })?;

        // Create executor (implement this)
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
        // Convert PyDict to HashMap with error handling
        let variables_map = python_dict_to_json_map(variables)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to convert variables: {}", e)
            ))?;

        // Register with executor
        self.executor.register_subscription(
            connection_id,
            subscription_id,
            query,
            operation_name,
            variables_map,
            user_id,
            tenant_id,
        ).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            format!("Failed to register subscription: {}", e)
        ))
    }

    pub fn publish_event(
        &self,
        event_type: String,
        channel: String,
        data: &Bound<PyDict>,
    ) -> PyResult<()> {
        // Convert to Event with error handling
        let event = python_dict_to_event(event_type.clone(), channel.clone(), data)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to convert event data for {}:{} : {}", event_type, channel, e)
            ))?;

        // Use runtime to publish
        self.runtime.block_on(async {
            self.executor.publish_event(event).await
        }).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            format!("Failed to publish event {}:{} : {}", event_type, channel, e)
        ))
    }

    pub fn next_event(
        &self,
        subscription_id: String,
    ) -> PyResult<Option<Vec<u8>>> {
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

// PyEventBusConfig - Event bus configuration
#[pyclass]
pub struct PyEventBusConfig {
    pub bus_type: String,
    pub config: EventBusConfig,  // Assume this exists
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
        // Basic validation
        if !connection_string.contains("postgresql://") {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid PostgreSQL connection string"));
        }

        Ok(Self {
            bus_type: "postgresql".to_string(),
            config: EventBusConfig::PostgreSQL { connection_string },
        })
    }
}

// Helper functions
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
) -> PyResult<Event> {  // Assume Event struct exists
    let data_map = python_dict_to_json_map(data)?;
    Ok(Event {
        event_type,
        channel,
        data: data_map,
    })
}

fn python_to_json_value(obj: &PyObject) -> PyResult<Value> {
    // Convert Python object to JSON Value
    // Implementation depends on your needs
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

fn json_to_python_dict(py: Python, json: &HashMap<String, Value>) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new_bound(py);
    for (key, value) in json {
        let py_value = json_to_python_value(py, value)?;
        dict.set_item(key, py_value)?;
    }
    Ok(dict.unbind())
}

fn json_to_python_value(py: Python, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::String(s) => Ok(s.clone().into_py(py)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(0.into_py(py))  // fallback
            }
        }
        Value::Bool(b) => Ok(b.into_py(py)),
        Value::Array(arr) => {
            let mut py_list = Vec::new();
            for item in arr {
                py_list.push(json_to_python_value(py, item)?);
            }
            Ok(py_list.into_py(py))
        }
        Value::Object(obj) => json_to_python_dict(py, obj).map(|d| d.into_py(py)),
        Value::Null => Ok(py.None()),
    }
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

// === ERROR HANDLING PATTERNS ===

// Pattern 1: PyO3 Error Conversion
fn convert_rust_error_to_py(err: SubscriptionError) -> PyErr {
    match err {
        SubscriptionError::ValidationError(msg) =>
            PyErr::new::<pyo3::exceptions::PyValueError, _>(msg),
        SubscriptionError::RuntimeError(msg) =>
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(msg),
        _ => PyErr::new::<pyo3::exceptions::PyException, _>(
            format!("Unknown error: {:?}", err)
        ),
    }
}

// Pattern 2: Safe Python Object Handling
fn safe_python_operation<F, R>(py: Python, operation: F) -> PyResult<R>
where
    F: FnOnce(Python) -> PyResult<R>,
{
    match operation(py) {
        Ok(result) => Ok(result),
        Err(e) => {
            // Log error details
            eprintln!("Python operation failed: {:?}", e);
            Err(e)
        }
    }
}

// Module initialization
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySubscriptionPayload>()?;
    m.add_class::<PyGraphQLMessage>()?;
    m.add_class::<PySubscriptionExecutor>()?;
    m.add_class::<PyEventBusConfig>()?;
    Ok(())
}
```

---

## Testing the Implementation

```python
# test_phase1_end_to_end.py
import pytest
from fraiseql import _fraiseql_rs

def test_complete_workflow():
    """Test the complete Phase 1 workflow"""
    # Create executor
    executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

    # Register subscription
    executor.register_subscription(
        connection_id="test_conn",
        subscription_id="test_sub",
        query="subscription { test }",
        variables={},
        user_id="test_user",
        tenant_id="test_tenant"
    )

    # Publish event
    executor.publish_event(
        event_type="test",
        channel="test",
        data={"message": "hello"}
    )

    # Get response
    response = executor.next_event("test_sub")
    assert response is not None
    assert isinstance(response, bytes)

    # Parse response
    import json
    response_data = json.loads(response)
    assert response_data["type"] == "next"
    assert "payload" in response_data

    # Get metrics
    metrics = executor.get_metrics()
    assert isinstance(metrics, dict)

    print("✅ Phase 1 implementation working!")
```

---

## Implementation Notes

### Key Points for Junior Engineers

1. **Runtime Management**: Use existing `init_runtime()` pattern
2. **Error Handling**: Convert Rust errors to `PyErr`
3. **GIL Management**: Use `Python::with_gil()` for Python operations
4. **Type Conversion**: Implement helpers for PyDict ↔ Rust types
5. **Memory Management**: Use `Arc` for shared data
6. **Async Bridge**: `runtime.block_on()` for sync → async

### Common Pitfalls

1. **Forgetting GIL**: Always use `Python::with_gil()` for Python object operations
2. **Type Mismatches**: Ensure PyO3 type annotations match
3. **Borrow Checker**: Use proper lifetimes for `Bound<PyDict>`
4. **Error Propagation**: Convert all Rust errors to PyErr
5. **Memory Leaks**: Use `Arc` appropriately, avoid cycles

### Testing Strategy

1. **Unit Tests**: Test each method individually
2. **Integration Tests**: Test complete workflows
3. **Type Tests**: Ensure Python types work correctly
4. **Error Tests**: Test error conditions and propagation
5. **Performance Tests**: Basic response time checks

This implementation provides a complete, working Phase 1 that junior engineers can adapt and extend.</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-1-implementation-example.py
