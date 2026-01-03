# Critical Fixes Action Plan

**Status**: Ready to implement
**Total Time**: ~10.5 hours
**Blocks**: Phase 1 implementation until complete
**Priority**: 🔴 CRITICAL - Must complete before Phase 1 starts

---

## Quick Summary

5 blocking issues found in planning documents. None affect the overall plan, but all must be fixed before junior engineers can successfully implement Phase 1.

**Impact if NOT fixed**: Junior engineers will be blocked for 2-3 hours per issue (~10 hours total wasted time during implementation)

---

## Issue #1: Phase-5 File Corrupted 🔴 CRITICAL

**File**: `.phases/graphQL-subscriptions-integration/phase-5.md`

**Problem**: Contains duplicate Phase 4 content instead of Phase 5 documentation planning

**Symptoms**:
- File discusses integration tests, performance benchmarks (Phase 4 work)
- No documentation, user guide, API reference content
- Junior engineer will have no guidance for final week

**Fix**: Rewrite phase-5.md with proper Phase 5 content

**Time**: 3-4 hours

### Detailed Fix

Replace entire `phase-5.md` with:

```markdown
# Phase 5: Documentation & Examples - Implementation Plan

**Phase**: 5
**Objective**: Create comprehensive user documentation
**Estimated Time**: 1 week / 20 hours
**Success Criteria**: User guide complete, API reference comprehensive, framework examples working

## Tasks

### 5.1: User Guide (10 hours)
**File**: `docs/subscriptions-guide.md` (~400 lines)

Sections needed:
- Introduction & key features
- Quick start (6 steps)
- Resolver development guide
- Framework integration (FastAPI, Starlette, custom)
- Event publishing patterns
- Client usage examples (JavaScript/Python)
- Troubleshooting guide
- Performance tips

### 5.2: API Reference (5 hours)
**File**: `docs/subscriptions-api-reference.md` (~300 lines)

Classes to document:
- `SubscriptionManager`
- `PySubscriptionExecutor`
- `WebSocketAdapter` (interface)
- `GraphQLTransportWSHandler`
- `PyEventBusConfig`

For each:
- Constructor signature
- All methods with parameters
- Return types
- Exceptions that can be raised
- Usage examples

### 5.3: Framework Integration Examples (5 hours)

Create working examples:
- `examples/subscriptions_fastapi.py` (~100 lines)
- `examples/subscriptions_starlette.py` (~100 lines)
- `examples/subscriptions_custom_server.py` (~100 lines)
- `examples/subscriptions_client.html` (~50 lines)

Each example:
- Complete working code
- Can be run as-is
- Comments explaining key concepts
- Real event publishing

## Acceptance Criteria

- [ ] User guide complete and reviewed
- [ ] API reference covers all public classes
- [ ] 3+ framework examples provided and tested
- [ ] Documentation builds without warnings
- [ ] Code examples run without errors
- [ ] README updated with subscriptions section
```

**Checklist**:
- [ ] Delete existing phase-5.md content
- [ ] Add correct Phase 5 sections above
- [ ] Create phase-5-checklist.md with same structure as other phases
- [ ] Verify no Phase 4 references remain

---

## Issue #2: SubscriptionData Struct Missing 🔴 CRITICAL

**File**: `.phases/graphQL-subscriptions-integration/_phase-1-implementation-guide.md` (Task 1.2)

**Problem**: References `SubscriptionData` struct but doesn't define it. Junior engineer won't know what fields to include.

**Symptoms**:
- Code says "Store subscription in executor" but doesn't show the data structure
- No guidance on field selection
- Junior engineer must guess or reverse-engineer from usage

**Fix**: Add struct definition to Phase 1.2

**Time**: 1 hour

### Detailed Fix

Add to Phase 1.2 section:

```rust
// Define in fraiseql_rs/src/subscriptions/executor.rs

pub struct SubscriptionData {
    /// Unique subscription identifier (from client)
    pub subscription_id: String,

    /// Connection ID (for cleanup on disconnect)
    pub connection_id: String,

    /// GraphQL subscription query string
    pub query: String,

    /// Operation name from query (e.g., "OnUserUpdated")
    pub operation_name: Option<String>,

    /// Variables passed with subscription
    pub variables: HashMap<String, serde_json::Value>,

    /// Python resolver function (called when event matches)
    /// SAFETY: Stored in Py<PyAny> to ensure GIL safety
    pub resolver_fn: Py<PyAny>,

    /// Security context for this subscription (user_id, tenant_id, permissions)
    pub security_context: Arc<SubscriptionSecurityContext>,

    /// Channels this subscription listens to (e.g., ["users", "posts"])
    pub channels: Vec<String>,

    /// Rate limiter for this subscription
    pub rate_limiter: Arc<RateLimiter>,

    /// When subscription was created
    pub created_at: std::time::SystemTime,

    /// Last time event was delivered (for monitoring)
    pub last_event_at: Option<std::time::SystemTime>,
}

impl SubscriptionData {
    pub fn new(
        subscription_id: String,
        connection_id: String,
        query: String,
        operation_name: Option<String>,
        variables: HashMap<String, serde_json::Value>,
        resolver_fn: Py<PyAny>,
        security_context: Arc<SubscriptionSecurityContext>,
        channels: Vec<String>,
    ) -> Self {
        Self {
            subscription_id,
            connection_id,
            query,
            operation_name,
            variables,
            resolver_fn,
            security_context,
            channels,
            rate_limiter: Arc::new(RateLimiter::new()),
            created_at: std::time::SystemTime::now(),
            last_event_at: None,
        }
    }
}
```

**Checklist**:
- [ ] Add struct definition with all fields documented
- [ ] Add constructor method
- [ ] Show field purposes in comments
- [ ] Reference this struct in register_subscription() example

---

## Issue #3: Resolver Storage (PyAny Lifetime) 🔴 CRITICAL

**File**: `.phases/graphQL-subscriptions-integration/_phase-1-implementation-guide.md` (Task 1.2)

**Problem**: Shows storing `resolver_fn: Py<PyAny>` but doesn't explain how to extract it from PyDict or call it later. Junior engineers unfamiliar with PyO3 will be confused.

**Symptoms**:
- "How do I extract Py<PyAny> from PyDict?"
- "When do I use Py<T> vs &T?"
- "How do I call a Python function from Rust?"
- GIL safety confusion

**Fix**: Add 3 explicit examples

**Time**: 1.5 hours

### Detailed Fix

Add to Phase 1.2 section under `register_subscription()`:

```rust
// EXAMPLE 1: Extracting resolver_fn from Python dict
// ─────────────────────────────────────────────────

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
    // Extract resolver function from variables dict
    // This is SAFE because Py<PyAny> holds a reference to the Python object
    // that stays alive as long as the Py<PyAny> exists

    let resolver_fn: Py<PyAny> = {
        // Must have GIL to interact with Python objects
        Python::with_gil(|py| {
            // Get "resolver_fn" from variables dict
            let resolver_obj = variables.get_item("resolver_fn")?;

            // Convert to Py<PyAny> (makes it safe to store in Rust)
            Py::from(resolver_obj)
        })
    };

    // Now resolver_fn is safe to store in Rust struct
    // It stays alive until we explicitly drop it

    let sub_data = SubscriptionData::new(
        subscription_id,
        connection_id,
        query,
        operation_name,
        python_dict_to_json_map(variables)?,
        resolver_fn,  // ← Stored safely here
        SubscriptionSecurityContext::new(user_id, tenant_id),
        vec![],  // channels added in Phase 2
    );

    // Store in executor's DashMap
    self.executor.subscriptions.insert(sub_data.subscription_id.clone(), sub_data);

    Ok(())
}

// EXAMPLE 2: Calling the Python resolver from Rust
// ──────────────────────────────────────────────────

fn invoke_python_resolver(
    &self,
    subscription_id: &str,
    resolver_fn: &Py<PyAny>,  // Reference to stored resolver
    event_data: &serde_json::Value,
    variables: &HashMap<String, serde_json::Value>,
) -> PyResult<serde_json::Value> {
    // CRITICAL: Must acquire GIL before calling Python function
    // This is a BLOCKING call (one per event per subscription)

    Python::with_gil(|py| {
        // Get the actual Python function object
        let py_resolver = resolver_fn.bind(py);

        // Convert Rust types to Python types
        let py_event = event_to_python_dict(py, event_data)?;
        let py_vars = json_to_python_dict(py, variables)?;

        // Call Python function: resolver_fn(event_dict, vars_dict)
        let result = py_resolver.call1((py_event, py_vars))?;

        // Convert Python result back to Rust JSON
        python_to_json_value(py, &result)
    })
}

// EXAMPLE 3: Understanding Py<PyAny> safety
// ───────────────────────────────────────────

// ❌ WRONG: Storing &PyAny directly
// let resolver_ref: &PyAny = variables.get_item("resolver_fn")?;
// store_in_rust_struct(resolver_ref);  // ERROR: lifetime too short!

// ✅ CORRECT: Using Py<PyAny>
let resolver_fn: Py<PyAny> = {
    Python::with_gil(|py| {
        let obj = variables.get_item("resolver_fn")?;
        Py::from(obj)  // Safe: reference counted, GIL-independent
    })
};
// Can now safely store in Rust struct forever

// KEY INSIGHT:
// - PyAny has a lifetime tied to Python::with_gil() scope
// - Py<PyAny> owns the reference and is safe to store long-term
// - Trade-off: must acquire GIL every time you want to use it
```

**Checklist**:
- [ ] Add Example 1 (extracting from PyDict)
- [ ] Add Example 2 (calling Python function)
- [ ] Add Example 3 (understanding safety)
- [ ] Add comments explaining GIL safety
- [ ] Reference these examples in Phase 2 dispatcher code

---

## Issue #4: Channel Index Missing 🔴 CRITICAL

**File**: `.phases/graphQL-subscriptions-integration/phase-2.md` (Task 2.1)

**Problem**: Event dispatcher needs to find "which subscriptions listen on channel X" but implementation not shown. Without this, O(n) scan of all subscriptions (unacceptable).

**Symptoms**:
- "How do I implement subscriptions_by_channel()?"
- "Where does channel_index live?"
- "When do I update it?"

**Fix**: Add channel index data structure and update `register_subscription()`

**Time**: 1.5 hours

### Detailed Fix

Add to Phase 2.1 section and update Phase 1.2:

```rust
// In SubscriptionExecutor struct (Phase 2.1)
pub struct SubscriptionExecutor {
    pub subscriptions: Arc<DashMap<String, SubscriptionData>>,

    /// NEW: Maps channel → set of subscription IDs listening on that channel
    /// Example: "users" → {"sub1", "sub2", "sub3"}
    pub channel_index: Arc<DashMap<String, HashSet<String>>>,

    pub event_bus: Arc<dyn EventBus>,
    pub response_queues: Arc<DashMap<String, Arc<tokio::sync::Mutex<VecDeque<Vec<u8>>>>>>,
    pub metrics: Arc<SecurityMetrics>,
}

// Implement channel lookup (Phase 2.2)
impl SubscriptionExecutor {
    /// Find all subscriptions listening on a channel
    /// Returns: Vec<subscription_id>
    fn subscriptions_by_channel(&self, channel: &str) -> Vec<String> {
        self.channel_index
            .get(channel)
            .map(|set_ref| {
                set_ref
                    .iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

// UPDATE register_subscription() (Phase 1.2)
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
    let resolver_fn: Py<PyAny> = { /* ... */ };

    // Create subscription data
    let sub_data = SubscriptionData::new(
        subscription_id.clone(),
        connection_id,
        query,
        operation_name,
        python_dict_to_json_map(variables)?,
        resolver_fn,
        SubscriptionSecurityContext::new(user_id, tenant_id),
        vec!["users", "posts"],  // Channels subscription listens on
    );

    // Store subscription
    self.subscriptions.insert(
        subscription_id.clone(),
        sub_data.clone(),
    );

    // UPDATE CHANNEL INDEX: Add this subscription to each channel
    for channel in &sub_data.channels {
        self.channel_index
            .entry(channel.clone())
            .or_insert_with(HashSet::new)
            .insert(subscription_id.clone());
    }

    Ok(())
}

// CLEANUP on subscription complete (Phase 2.3 or 3)
pub fn complete_subscription(&self, subscription_id: &str) -> Result<(), SubscriptionError> {
    // Remove from subscriptions
    if let Some((_, sub_data)) = self.subscriptions.remove(subscription_id) {
        // Remove from channel_index
        for channel in &sub_data.channels {
            if let Some(mut set_ref) = self.channel_index.get_mut(channel) {
                set_ref.remove(subscription_id);
                // Clean up empty entries to prevent memory leak
                if set_ref.is_empty() {
                    drop(set_ref);
                    self.channel_index.remove(channel);
                }
            }
        }
    }

    Ok(())
}
```

**Performance note**:
- O(1) channel lookup: Direct DashMap get
- O(n) subscription processing where n = subscriptions on that channel
- O(n) cleanup on complete_subscription

**Checklist**:
- [ ] Add `channel_index` field to SubscriptionExecutor
- [ ] Implement `subscriptions_by_channel()` method
- [ ] Update `register_subscription()` to maintain index
- [ ] Add `complete_subscription()` cleanup
- [ ] Add comment explaining why channel_index is needed

---

## Issue #5: EventBus Creation Missing 🔴 CRITICAL

**File**: `.phases/graphQL-subscriptions-integration/_phase-1-implementation-guide.md` (Task 1.3)

**Problem**: Phase 1.3 shows creating PyEventBusConfig, but nowhere shows creating actual EventBus instance from config. Junior engineer won't know how to instantiate the event bus.

**Symptoms**:
- "How do I create a Redis event bus from PyEventBusConfig?"
- "Where does the EventBus go?"
- "How does dispatcher get access to it?"

**Fix**: Add `create_bus()` method to PyEventBusConfig

**Time**: 1 hour

### Detailed Fix

Add to Phase 1.3 section:

```rust
// In fraiseql_rs/src/subscriptions/py_bindings.rs (Phase 1.3)

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
        // Validate URL format
        if !url.starts_with("redis://") && !url.starts_with("rediss://") {
            return Err(PyErr::new::<PyValueError, _>("URL must start with redis:// or rediss://"));
        }

        Ok(Self {
            bus_type: "redis".to_string(),
            config: EventBusConfig::Redis {
                url,
                consumer_group,
            },
        })
    }

    #[staticmethod]
    pub fn postgresql(connection_string: String) -> PyResult<Self> {
        // Validate connection string
        if connection_string.is_empty() {
            return Err(PyErr::new::<PyValueError, _>("Connection string cannot be empty"));
        }

        Ok(Self {
            bus_type: "postgresql".to_string(),
            config: EventBusConfig::PostgreSQL {
                connection_string,
            },
        })
    }

    /// NEW: Create actual EventBus instance from config
    pub fn create_bus(&self) -> PyResult<Arc<dyn EventBus>> {
        // NOTE: Async operations moved to Phase 2
        // For now, only InMemory works synchronously

        match &self.config {
            EventBusConfig::InMemory => {
                Ok(Arc::new(InMemoryEventBus::new()))
            }
            EventBusConfig::Redis { url, consumer_group } => {
                // Redis requires async connection
                // This is handled in SubscriptionExecutor::new() in Phase 2
                Err(PyErr::new::<PyRuntimeError, _>(
                    "Redis EventBus requires async initialization in Phase 2"
                ))
            }
            EventBusConfig::PostgreSQL { connection_string } => {
                // PostgreSQL requires async connection
                // This is handled in SubscriptionExecutor::new() in Phase 2
                Err(PyErr::new::<PyRuntimeError, _>(
                    "PostgreSQL EventBus requires async initialization in Phase 2"
                ))
            }
        }
    }

    #[getter]
    pub fn bus_type(&self) -> String {
        self.bus_type.clone()
    }
}

// In SubscriptionExecutor (Phase 2.1)
#[pyclass]
pub struct PySubscriptionExecutor {
    executor: Arc<SubscriptionExecutor>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PySubscriptionExecutor {
    #[new]
    pub fn new_with_config(config: PyEventBusConfig) -> PyResult<Self> {
        let runtime = Arc::new(crate::db::runtime::get_runtime()?);

        // For InMemory, create immediately
        let event_bus = match &config.config {
            EventBusConfig::InMemory => {
                Arc::new(InMemoryEventBus::new()) as Arc<dyn EventBus>
            }
            // For Redis/PostgreSQL, use async initialization
            other => {
                // Create async in tokio runtime
                let event_bus = runtime.block_on(async {
                    match other {
                        EventBusConfig::Redis { url, consumer_group } => {
                            RedisEventBus::connect(url.clone(), consumer_group.clone())
                                .await
                                .map(|bus| Arc::new(bus) as Arc<dyn EventBus>)
                                .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))
                        }
                        EventBusConfig::PostgreSQL { connection_string } => {
                            PostgreSQLEventBus::connect(connection_string.clone())
                                .await
                                .map(|bus| Arc::new(bus) as Arc<dyn EventBus>)
                                .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))
                        }
                        EventBusConfig::InMemory => {
                            Ok(Arc::new(InMemoryEventBus::new()) as Arc<dyn EventBus>)
                        }
                    }
                })?;
                event_bus
            }
        };

        let executor = Arc::new(SubscriptionExecutor::new_with_bus(event_bus));

        Ok(Self {
            executor,
            runtime,
        })
    }
}
```

**Checklist**:
- [ ] Add `create_bus()` method to PyEventBusConfig
- [ ] Show EventBus creation in PySubscriptionExecutor::new()
- [ ] Handle async initialization in Phase 2
- [ ] Document that Redis/PostgreSQL need async setup
- [ ] Add error handling for invalid configs

---

## Implementation Order

1. **Fix #1** - Rewrite phase-5.md (4 hours)
   - Unblocks documentation guidance
   - Can be done in parallel

2. **Fix #2** - Add SubscriptionData struct (1 hour)
   - Required before Phase 1.2 implementation
   - Must be done first

3. **Fix #3** - Add resolver storage examples (1.5 hours)
   - Required for Phase 1.2 junior engineer clarity
   - Must be done before Phase 1 starts

4. **Fix #4** - Add channel index (1.5 hours)
   - Required before Phase 2.2 implementation
   - Can be done in parallel with Phase 1

5. **Fix #5** - Add EventBus creation (1 hour)
   - Required for Phase 2 executor setup
   - Can be done in parallel with Phase 1

**Parallel path**:
- Do Fix #1 in parallel (4 hours)
- Do Fixes #2-3 sequentially (2.5 hours) - blocks Phase 1
- Do Fixes #4-5 in parallel (2.5 hours)

**Total time**: 4 hours (parallel) + 2.5 hours (sequential) + 2.5 hours (parallel) = ~6-7 hours in practice

---

## Success Criteria

After applying all fixes:

- [ ] Phase 1.2 junior engineer can implement without questions about resolver storage
- [ ] Phase 1.2 junior engineer knows exact SubscriptionData struct fields
- [ ] Phase 2.2 junior engineer can implement channel_index without asking how
- [ ] Phase 2 junior engineer can access EventBus without asking where it comes from
- [ ] Phase 5 has complete documentation guidance
- [ ] No duplicate content in phase files
- [ ] All code examples compile (checked during Phase 1 implementation)

---

## Verification Checklist

When fixes are complete:

- [ ] phase-5.md contains documentation tasks, NOT Phase 4 content
- [ ] SubscriptionData struct fully documented in Phase 1.2
- [ ] 3 explicit examples for Py<PyAny> resolver storage in Phase 1.2
- [ ] channel_index field and implementation in Phase 2.1
- [ ] subscriptions_by_channel() implementation shown
- [ ] complete_subscription() cleanup shown
- [ ] EventBus creation in Phase 1.3
- [ ] PySubscriptionExecutor::new() uses EventBusConfig
- [ ] No conflicting information between phases
- [ ] All code examples follow existing patterns

---

## Timeline

**Week 0**: Apply critical fixes (before Phase 1 starts)
- Mon-Wed: Fixes #1-3 (prepare Phase 1 materials)
- Thu-Fri: Fixes #4-5 (prepare Phase 2 materials)

**Week 1**: Phase 1 starts (with fixes applied)

---

**Status**: Ready to implement
**Estimated Total Time**: 10.5 hours
**Blocks**: Phase 1 implementation
**Priority**: 🔴 CRITICAL - Apply before implementation starts
