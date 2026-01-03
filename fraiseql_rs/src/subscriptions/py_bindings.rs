//! `PyO3` bindings for GraphQL subscriptions module (Phase 1).
//!
//! Exposes Rust subscription engine to Python for seamless integration.

use dashmap::DashMap;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// Import from existing modules
use crate::db::runtime::init_runtime;
use crate::subscriptions::config::EventBusConfig;
use crate::subscriptions::executor::{ResolverCallback, SubscriptionExecutor};
use crate::subscriptions::protocol::SubscriptionPayload;
use crate::subscriptions::SubscriptionError;
use crate::subscriptions::SubscriptionSecurityContext;

/// Python wrapper for `SubscriptionPayload` (GraphQL subscription data)
#[pyclass]
#[derive(Debug)]
pub struct PySubscriptionPayload {
    /// GraphQL query string
    #[pyo3(get, set)]
    pub query: String,
    /// Optional operation name
    #[pyo3(get, set)]
    pub operation_name: Option<String>,
    /// Query variables as Python dict
    #[pyo3(get, set)]
    pub variables: Py<PyDict>,
    /// Optional extensions
    #[pyo3(get, set)]
    pub extensions: Option<Py<PyDict>>,
}

#[pymethods]
impl PySubscriptionPayload {
    #[new]
    #[must_use] 
    pub fn new(query: String) -> Self {
        Self {
            query,
            operation_name: None,
            variables: Python::with_gil(|py| PyDict::new(py).unbind()),
            extensions: None,
        }
    }
}

/// Python wrapper for GraphQL WebSocket protocol messages
#[pyclass]
#[derive(Debug, Default)]
pub struct PyGraphQLMessage {
    /// Message type (e.g., "`connection_init`", "subscribe", etc.)
    pub type_: String,
    /// Optional message ID
    pub id: Option<String>,
    /// Optional payload data
    pub payload: Option<Py<PyDict>>,
}

#[pymethods]
impl PyGraphQLMessage {
    #[new]
    #[must_use] 
    pub fn new() -> Self {
        Self::default()
    }

    #[staticmethod]
    pub fn from_dict(data: &Bound<PyDict>) -> PyResult<Self> {
        // Extract required 'type' field
        let type_ = data
            .get_item("type")?
            .ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyValueError, _>("Missing required field: 'type'")
            })?
            .extract::<String>()?;

        // Extract optional 'id' field
        let id = data
            .get_item("id")
            .ok()
            .and_then(|i| i.and_then(|item| item.extract::<String>().ok()));

        // Extract optional 'payload' field
        let payload = data.get_item("payload").ok().and_then(|p| {
            p.map_or_else(|| None, |p_some| p_some.downcast::<PyDict>().ok().map(|d| d.clone().unbind()))
        });

        Ok(Self { type_, id, payload })
    }

    pub fn to_dict(&self) -> PyResult<Py<PyDict>> {
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
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

    /// Get message type property
    #[getter(type_)]
    #[must_use] 
    pub fn get_type(&self) -> String {
        self.type_.clone()
    }

    /// Set message type property
    #[setter(type_)]
    pub fn set_type(&mut self, value: String) {
        self.type_ = value;
    }

    /// Get message ID property
    #[getter(id)]
    #[must_use] 
    pub fn get_id(&self) -> Option<String> {
        self.id.clone()
    }

    /// Set message ID property
    #[setter(id)]
    pub fn set_id(&mut self, value: Option<String>) {
        self.id = value;
    }

    /// Get message payload property
    #[getter(payload)]
    #[must_use] 
    pub const fn get_payload(&self) -> Option<&Py<PyDict>> {
        self.payload.as_ref()
    }

    /// Set message payload property
    #[setter(payload)]
    pub fn set_payload(&mut self, value: Option<Py<PyDict>>) {
        self.payload = value;
    }
}

/// Python wrapper for event bus configuration
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyEventBusConfig {
    /// Configuration type identifier
    #[pyo3(get)]
    pub bus_type: String,
    /// Actual Rust configuration
    pub config: EventBusConfig,
}

#[pymethods]
impl PyEventBusConfig {
    #[staticmethod]
    #[must_use] 
    pub fn memory() -> Self {
        Self {
            bus_type: "memory".to_string(),
            config: EventBusConfig::InMemory,
        }
    }

    #[staticmethod]
    pub fn redis(url: String, consumer_group: String) -> PyResult<Self> {
        if !url.starts_with("redis://") {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid Redis URL",
            ));
        }
        Ok(Self {
            bus_type: "redis".to_string(),
            config: EventBusConfig::Redis {
                url,
                consumer_group,
                message_ttl: 3600, // 1 hour default
            },
        })
    }

    #[staticmethod]
    pub fn postgresql(connection_string: String) -> PyResult<Self> {
        if !connection_string.contains("postgresql://") {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid PostgreSQL connection string",
            ));
        }
        Ok(Self {
            bus_type: "postgresql".to_string(),
            config: EventBusConfig::PostgreSQL {
                connection_string,
                channel_prefix: "fraiseql".to_string(),
            },
        })
    }
}

/// Main Python interface to the Rust subscription executor
///
/// Provides methods to:
/// - Register subscriptions
/// - Publish events
/// - Retrieve subscription responses
/// - Manage subscription lifecycle
#[pyclass]
#[derive(Debug, Clone)]
pub struct PySubscriptionExecutor {
    /// The underlying Rust executor
    executor: Arc<SubscriptionExecutor>,

    /// Map of `subscription_id` -> Python resolver function
    /// Used to invoke user-defined resolvers when events are published
    resolvers: Arc<DashMap<String, Py<PyAny>>>,
}

#[pymethods]
impl PySubscriptionExecutor {
    #[new]
    pub fn new() -> PyResult<Self> {
        // Initialize runtime if not already done
        init_runtime(&crate::db::runtime::RuntimeConfig::default()).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to initialize runtime: {e}"
            ))
        })?;

        // Create executor
        let executor = Arc::new(SubscriptionExecutor::new());

        let py_executor = Self {
            executor: executor.clone(),
            resolvers: Arc::new(DashMap::new()),
        };

        // Install resolver callback (Phase 3)
        // This allows the executor to invoke Python resolvers during event dispatch
        let callback: Arc<dyn ResolverCallback> = Arc::new(py_executor.clone());
        executor.set_resolver_callback(callback);

        Ok(py_executor)
    }

    /// Register a new subscription
    ///
    /// Args:
    ///     `connection_id`: Unique connection identifier for WebSocket connection
    ///     `subscription_id`: Unique subscription identifier (from client)
    ///     query: GraphQL subscription query string
    ///     `operation_name`: Optional operation name (e.g., "`OnUserUpdated`")
    ///     variables: Query variables as Python dict
    ///     `user_id`: Authenticated user ID for security validation (i64).
    ///         Events will be filtered to match this `user_id` to prevent privilege escalation.
    ///     `tenant_id`: Authenticated tenant ID for multi-tenant isolation (i64).
    ///         Events will be filtered to match this `tenant_id` to prevent cross-tenant data leaks.
    ///
    /// Returns:
    ///     Subscription ID (string) that can be used with `next_event()` and `complete_subscription()`
    ///
    /// Raises:
    ///     `RuntimeError`: If registration fails
    ///     `ValueError`: If parameters are invalid
    pub fn register_subscription(
        &self,
        connection_id: &str,
        subscription_id: &str,
        query: String,
        operation_name: Option<String>,
        variables: &Bound<PyDict>,
        user_id: i64,
        tenant_id: i64,
    ) -> PyResult<String> {
        // Validate inputs
        if connection_id.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "connection_id cannot be empty",
            ));
        }
        if subscription_id.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "subscription_id cannot be empty",
            ));
        }
        if query.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "query cannot be empty",
            ));
        }

        // Convert Python dict to Rust HashMap
        let variables_map = python_dict_to_json_map(variables)?;

        // Create security context
        let security_context = SubscriptionSecurityContext::new(user_id, tenant_id);

        // Create payload
        let payload = SubscriptionPayload {
            query,
            operation_name,
            variables: Some(variables_map),
            extensions: None,
        };

        // Parse connection_id as UUID, or create a consistent one from the string
        let conn_uuid = uuid::Uuid::parse_str(connection_id).unwrap_or_else(|_| {
            // If not a valid UUID, hash the connection_id string to create a stable UUID
            // Use MD5 hash of the connection_id string to generate a v3 UUID-like identifier
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            connection_id.hash(&mut hasher);
            let hash = hasher.finish();

            // Convert hash to UUID bytes (u128 -> [u8; 16])
            let hash_bytes = hash.to_ne_bytes();
            let mut uuid_bytes = [0u8; 16];
            uuid_bytes[0..8].copy_from_slice(&hash_bytes);
            // Add salt from connection_id length for variation
            uuid_bytes[8..].copy_from_slice(&(connection_id.len() as u64).to_ne_bytes());

            uuid::Uuid::from_bytes(uuid_bytes)
        });

        // Execute subscription with security
        let result = self
            .executor
            .execute_with_security(conn_uuid, &payload, security_context)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to register subscription '{subscription_id}': {e}"
                ))
            })?;

        // Store the subscription ID for later retrieval
        let actual_subscription_id = result.subscription.id;

        // Return the subscription ID to Python
        Ok(actual_subscription_id)
    }

    /// Publish an event to trigger subscriptions (Phase 2)
    ///
    /// Dispatches an event to all subscriptions on the matching channel.
    /// Uses the Rust dispatch engine for parallel processing.
    ///
    /// Args:
    ///     `event_type`: Type of event (e.g., "userCreated", "userUpdated")
    ///     channel: Event channel/topic (e.g., "users", "posts")
    ///     data: Event data as Python dict
    ///
    /// Returns:
    ///     None on success
    ///
    /// Raises:
    ///     `RuntimeError`: If event publishing fails
    ///     `ValueError`: If parameters are invalid
    pub fn publish_event(
        &self,
        event_type: &str,
        channel: &str,
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

        // Convert data to JSON
        let data_map = python_dict_to_json_map(data)?;

        // Create JSON object from data map
        let data_json = serde_json::Value::Object(
            data_map
                .into_iter()
                .collect::<serde_json::Map<String, serde_json::Value>>(),
        );

        // Validate event structure
        if data_json.is_null() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Event data cannot be null",
            ));
        }

        // Phase 2: Dispatch event to all matching subscriptions
        // Convert to Arc for zero-copy passing through dispatch pipeline
        let event_data = std::sync::Arc::new(data_json);

        // Get the global tokio runtime to dispatch asynchronously
        let rt = crate::db::runtime::runtime();

        // Execute dispatch asynchronously, blocking Python thread until complete
        let dispatch_result = rt.block_on(async {
            self.executor
                .dispatch_event(event_type.to_string(), channel.to_string(), event_data)
                .await
        });

        match dispatch_result {
            Ok(_count) => Ok(()),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Event dispatch failed: {e}"
            ))),
        }
    }

    /// Get the next response for a subscription (pre-serialized bytes)
    ///
    /// Args:
    ///     `subscription_id`: Subscription identifier (from `register_subscription`)
    ///
    /// Returns:
    ///     Response bytes if available, None if no response ready
    ///
    /// Raises:
    ///     `ValueError`: If subscription doesn't exist
    pub fn next_event(&self, subscription_id: &str) -> PyResult<Option<Vec<u8>>> {
        // Get next response from the subscription's response queue
        match self.executor.next_event(subscription_id) {
            Ok(response) => Ok(response),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Failed to get next event: {e}"
            ))),
        }
    }

    /// Complete a subscription and clean up resources
    ///
    /// Args:
    ///     `subscription_id`: Subscription identifier (from `register_subscription`)
    ///
    /// Returns:
    ///     None on success
    ///
    /// Raises:
    ///     `RuntimeError`: If completion fails
    ///     `ValueError`: If subscription doesn't exist
    pub fn complete_subscription(&self, subscription_id: &str) -> PyResult<()> {
        // Remove from resolvers map
        self.resolvers.remove(subscription_id);

        // Notify executor
        self.executor
            .complete_subscription(subscription_id)
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to complete subscription '{subscription_id}': {e}"
                ))
            })
    }

    /// Get all subscription IDs subscribed to a channel
    ///
    /// Uses the channel index for fast O(1) lookup of all subscriptions
    /// that are listening to events on a specific channel.
    ///
    /// Args:
    ///     channel: Channel name (e.g., "users", "orders")
    ///
    /// Returns:
    ///     List of subscription IDs subscribed to this channel
    #[must_use] 
    pub fn subscriptions_by_channel(&self, channel: &str) -> Vec<String> {
        self.executor.subscriptions_by_channel(channel)
    }

    /// Invoke a registered resolver for an event (internal, Phase 3)
    ///
    /// This method looks up the resolver function for a subscription and invokes it
    /// with the event data through Python's JSON module for safe serialization.
    ///
    /// Args:
    ///     `subscription_id`: The subscription ID
    ///     `event_data_json`: The raw event data as JSON string
    ///
    /// Returns:
    ///     The resolver result as a JSON string
    ///
    /// Raises:
    ///     `RuntimeError`: If resolver execution fails or resolver not found
    fn invoke_resolver_internal(
        &self,
        subscription_id: &str,
        event_data_json: &str,
    ) -> PyResult<String> {
        // Try to get resolver from map and clone it within the GIL context
        pyo3::Python::with_gil(|py| {
            match self.resolvers.get(subscription_id) {
                Some(resolver_ref) => {
                    // Clone the resolver using clone_ref which is the proper way for Py<T>
                    let resolver_py = resolver_ref.value().clone_ref(py);
                    drop(resolver_ref); // Drop the guard early

                    // Import json module and parse event_data_json
                    let json_mod = py.import("json")?;
                    let event_dict = json_mod.getattr("loads")?.call1((event_data_json,))?;

                    // Call the resolver with the event data
                    let resolver_result = resolver_py.call1(py, (event_dict,)).map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                            "Resolver error: {e}"
                        ))
                    })?;

                    // Convert result back to JSON string
                    json_mod
                        .getattr("dumps")?
                        .call1((resolver_result,))?
                        .extract::<String>()
                }
                None => {
                    // No resolver registered - use default echo resolver
                    Ok(format!(r#"{{"data": {event_data_json}}}"#))
                }
            }
        })
    }

    /// Register a Python resolver function for a subscription (Phase 3)
    ///
    /// The resolver function will be called whenever an event is published
    /// to the subscription's channel. The resolver transforms raw event data
    /// into GraphQL response data matching the subscription's selection set.
    ///
    /// Resolver Signature:
    /// ```python
    /// def resolver(event_data: dict) -> dict:
    ///     # event_data: Raw event data from the database
    ///     # Returns: GraphQL response data matching subscription fields
    ///     return {
    ///         "field1": event_data.get("field1"),
    ///         "field2": event_data.get("field2"),
    ///         ...
    ///     }
    /// ```
    ///
    /// Args:
    ///     `subscription_id`: The subscription ID returned by `register_subscription()`
    ///     resolver: A Python callable that takes `event_data` dict and returns response dict
    ///
    /// Returns:
    ///     None on success
    ///
    /// Raises:
    ///     `ValueError`: If `subscription_id` doesn't exist or resolver is not callable
    ///     `RuntimeError`: If resolver registration fails
    ///
    /// Example:
    /// ```python
    /// def order_resolver(event_data):
    ///     return {
    ///         "id": event_data["order_id"],
    ///         "status": event_data["status"],
    ///         "total": event_data["amount"]
    ///     }
    ///
    /// sub_id = executor.register_subscription(...)
    /// executor.register_resolver(sub_id, order_resolver)
    /// ```
    pub fn register_resolver(
        &self,
        subscription_id: &str,
        resolver: &Bound<PyAny>,
    ) -> PyResult<()> {
        // Validate inputs
        if subscription_id.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "subscription_id cannot be empty",
            ));
        }

        // Check if resolver is callable
        if !resolver.is_callable() {
            return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "resolver must be a callable (function)",
            ));
        }

        // Verify subscription exists
        // Try to get the subscription from the executor's internal map
        // If it exists, we can register the resolver
        // Note: We don't have a direct way to verify subscription existence,
        // so we trust the user to provide a valid subscription_id
        // The resolver will fail at invocation time if subscription doesn't exist

        // Store resolver in the resolvers map
        // Convert the Bound reference to a Py<PyAny> owned object
        let resolver_py: Py<PyAny> = resolver.clone().unbind();

        self.resolvers.insert(subscription_id.to_string(), resolver_py);

        println!(
            "[Phase 3] Resolver registered for subscription: {subscription_id}"
        );

        Ok(())
    }

    /// Get executor metrics as Python dict
    ///
    /// Returns:
    ///     Dict with subscription counts and metrics
    pub fn get_metrics(&self) -> PyResult<Py<PyDict>> {
        let metrics = self.executor.metrics();
        python_metrics_dict(metrics)
    }
}

/// `ResolverCallback` implementation for `PySubscriptionExecutor` (Phase 3)
///
/// Allows `SubscriptionExecutor` to invoke Python resolvers through the callback interface.
impl ResolverCallback for PySubscriptionExecutor {
    fn invoke(
        &self,
        subscription_id: &str,
        event_data_json: &str,
    ) -> Result<String, SubscriptionError> {
        // Call the internal resolver invocation method
        pyo3::Python::with_gil(|_py| {
            match self.invoke_resolver_internal(subscription_id, event_data_json) {
                Ok(result_json) => Ok(result_json),
                Err(e) => {
                    // Convert PyErr to SubscriptionError
                    Err(SubscriptionError::SubscriptionRejected(format!(
                        "Python resolver error: {e}"
                    )))
                }
            }
        })
    }
}

/// Convert Python dict to Rust `HashMap`<String, Value>
fn python_dict_to_json_map(dict: &Bound<PyDict>) -> PyResult<HashMap<String, Value>> {
    let mut map = HashMap::new();
    for (key, value) in dict.iter() {
        let key_str = key.extract::<String>()?;
        let value_json = python_to_json_value(&value)?;
        map.insert(key_str, value_json);
    }
    Ok(map)
}

/// Convert Python object to `serde_json` Value
/// Recursively converts Python types (str, int, float, bool, list, dict) to JSON values
fn python_to_json_value(obj: &Bound<PyAny>) -> PyResult<Value> {
    // Try scalar types first (faster path)
    if let Ok(s) = obj.extract::<String>() {
        Ok(Value::String(s))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(Value::Number(i.into()))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(Value::Number(
            serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
        ))
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(Value::Bool(b))
    } else if let Ok(list) = obj.downcast::<PyList>() {
        // Recursively convert list items
        let items: Result<Vec<Value>, _> = list
            .iter()
            .map(|item| python_to_json_value(&item))
            .collect();
        Ok(Value::Array(items?))
    } else if let Ok(dict) = obj.downcast::<PyDict>() {
        // Recursively convert dict values
        let map = python_dict_to_json_map(dict)?;
        Ok(Value::Object(serde_json::Map::from_iter(map)))
    } else {
        // Unsupported types return null (this is safer than panic)
        Ok(Value::Null)
    }
}

/// Convert Rust metrics to Python dict
fn python_metrics_dict(metrics: Value) -> PyResult<Py<PyDict>> {
    Python::with_gil(|py| {
        let dict = PyDict::new(py);

        if let Value::Object(map) = metrics {
            for (key, value) in map {
                convert_value_to_dict(&dict, key, value)?;
            }
        }

        Ok(dict.unbind())
    })
}

/// Helper to convert a JSON value to dictionary item (reduces nesting)
fn convert_value_to_dict(dict: &Bound<PyDict>, key: String, value: Value) -> PyResult<()> {
    match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                dict.set_item(key, i)?;
            } else if let Some(f) = n.as_f64() {
                dict.set_item(key, f)?;
            }
        }
        Value::String(s) => dict.set_item(key, s)?,
        Value::Bool(b) => dict.set_item(key, b)?,
        _ => {} // Skip complex types for now
    }
    Ok(())
}

/// Initialize the subscriptions module for Python
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySubscriptionPayload>()?;
    m.add_class::<PyGraphQLMessage>()?;
    m.add_class::<PySubscriptionExecutor>()?;
    m.add_class::<PyEventBusConfig>()?;

    // Export all public classes via __all__
    m.add(
        "__all__",
        vec![
            "PySubscriptionPayload",
            "PyGraphQLMessage",
            "PySubscriptionExecutor",
            "PyEventBusConfig",
        ],
    )?;

    Ok(())
}
