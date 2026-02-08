# GraphQL Subscriptions Python Integration - V2
## Performance-First, Rust-Heavy Architecture

**Date**: January 3, 2026
**Status**: Detailed Planning Phase
**Philosophy**: Maximum Rust, Minimal Python, User writes only Python code
**Target Performance**: <10ms E2E latency (database event → subscription message)

---

## 🎯 Core Design Philosophy

### Principle 1: Everything Fast Happens in Rust
- Event bus operations (publish/subscribe)
- Security validation (all 5 modules)
- Event filtering and routing
- Connection lifecycle
- Rate limiting and metrics

### Principle 2: Python Only for Declaration
- User writes: `@subscription` decorator
- User writes: Query and resolver
- User writes: Connection setup
- Framework handles: All event distribution (Rust)

### Principle 3: Zero-Copy Data Movement
- Events wrapped in `Arc<Event>` (pointer copying, not data)
- Responses pre-serialized to bytes (no dict conversion)
- No intermediate JSON parse/serialize cycles
- Direct Arc passing through async boundaries

### Principle 4: Leverage Existing Infrastructure
- Use existing Tokio runtime (shared, already configured)
- Use existing RustResponseBytes pattern
- Extend existing EventBus trait
- Reuse SubscriptionSecurityContext (5 modules integrated)

---

## Architecture: Rust-Heavy Distribution Network

```
USER CODE (Python)
├── @subscription decorator
├── Resolver function (async/sync)
└── GraphQL query definition

        ↓ (Registration only, not runtime)

SUBSCRIPTION REGISTRY (Rust)
├── Store: subscription_id → (query, resolver_fn, security_ctx)
├── Manage: active subscriptions per connection
└── Validate: security context once at subscription time

        ↓ (Event notification)

EVENT BUS (Rust) - ASYNC CORE
├── Redis backend (production)
├── PostgreSQL backend (fallback)
└── In-Memory backend (testing)

        ↓ (Zero-copy Arc<Event>)

SUBSCRIPTION EXECUTOR (Rust)
├── Match event to subscriptions (channel filtering)
├── Apply SecurityAwareEventFilter per subscription
├── Apply RateLimiter per user/subscription
├── Invoke Python resolver (blocking call)

        ↓ (Resolver result only)

RESPONSE ENCODER (Rust)
├── Convert Python resolver result
├── Apply __typename injection
├── Serialize to RustResponseBytes
└── Return pre-serialized JSON bytes

        ↓ (Pre-serialized bytes)

WEBSOCKET LAYER (Python FastAPI)
├── Send bytes directly to client
├── Manage connection keep-alive
└── Handle disconnections
```

---

## Implementation Strategy: 5 Phases

### PHASE 1: PyO3 Core Bindings (2 weeks, 30 hours)
**Goal**: Expose Rust engine to Python with minimal overhead

#### 1.1 Subscription Payload Types (6 hours)
```rust
// fraiseql_rs/src/subscriptions/py_bindings.rs

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
    pub fn new(query: String) -> Self { ... }
}

// Protocol messages (graphql-transport-ws)
#[pyclass]
pub struct PyGraphQLMessage {
    #[pyo3(get)]
    pub type_: String,  // "connection_init", "subscribe", "next", "error", "complete"
    #[pyo3(get)]
    pub id: Option<String>,
    #[pyo3(get)]
    pub payload: Option<Py<PyDict>>,
}

#[pymethods]
impl PyGraphQLMessage {
    #[staticmethod]
    pub fn from_dict(data: &Bound<PyDict>) -> PyResult<Self> { ... }
    pub fn to_dict(&self) -> Py<PyDict> { ... }
}
```

**Strategy**: Keep these minimal - just type stubs for passing between Python and Rust.

#### 1.2 Core Subscription Executor (8 hours)
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
        // Use existing global runtime from db::runtime
        let runtime = Arc::new(crate::db::runtime::runtime().clone());
        Self {
            executor: Arc::new(SubscriptionExecutor::new()),
            runtime,
        }
    }

    // CRITICAL: Register subscription (blocking Python call)
    // This stores subscription in Rust but doesn't start async work yet
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
        // Convert Python dict variables to Rust HashMap<String, Value>
        let vars = python_dict_to_json_map(variables)?;

        // Create security context
        let security_ctx = SubscriptionSecurityContext::new(user_id, tenant_id);

        // Store subscription in executor (non-async)
        self.executor.register_subscription(
            connection_id,
            subscription_id,
            query,
            operation_name,
            vars,
            security_ctx,
        )?;

        Ok(())
    }

    // Publish event (blocking call, internally async)
    pub fn publish_event(
        &self,
        event_type: String,
        channel: String,
        data: &Bound<PyDict>,
    ) -> PyResult<()> {
        // Convert Python dict to Arc<Event>
        let event = python_dict_to_event(event_type, channel, data)?;

        // Block on async publish
        self.runtime.block_on(async {
            self.executor.publish_event(event).await
        })?;

        Ok(())
    }

    // Get next event for subscription (blocking call)
    // Returns pre-serialized bytes (RustResponseBytes pattern)
    pub fn next_event(
        &self,
        subscription_id: String,
    ) -> PyResult<Option<Vec<u8>>> {
        let result = self.runtime.block_on(async {
            self.executor.get_next_event(&subscription_id).await
        })?;

        Ok(result)  // Already serialized bytes
    }

    // Complete subscription cleanup
    pub fn complete_subscription(&self, subscription_id: String) -> PyResult<()> {
        self.executor.complete_subscription(&subscription_id)?;
        Ok(())
    }

    // Metrics (non-blocking)
    pub fn get_metrics(&self) -> Py<PyDict> {
        let metrics = self.executor.get_metrics();
        // Convert Rust metrics to Python dict
        let py_metrics = python_metrics_dict(metrics);
        Ok(py_metrics)
    }
}
```

**Key Design**:
- `register_subscription()` is fast (just stores in DashMap)
- `publish_event()` does async work via `block_on()`
- `next_event()` returns pre-serialized bytes (not dict)
- All heavy lifting stays in Rust async

#### 1.3 Event Bus Bridge (6 hours)
```rust
#[pyclass]
pub struct PyEventBusConfig {
    pub bus_type: String,  // "memory", "redis", "postgresql"
    pub config: EventBusConfig,
}

#[pymethods]
impl PyEventBusConfig {
    #[staticmethod]
    pub fn memory() -> Self { ... }

    #[staticmethod]
    pub fn redis(url: String, consumer_group: String) -> PyResult<Self> { ... }

    #[staticmethod]
    pub fn postgresql(connection_string: String) -> PyResult<Self> { ... }
}

// Note: Don't expose EventBus directly to Python
// Instead, wrap in executor which manages lifecycle
```

#### 1.4 Module Registration (5 hours)
Update `fraiseql_rs/src/lib.rs`:
```rust
// Add to PyModule
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<py_bindings::PySubscriptionPayload>()?;
    m.add_class::<py_bindings::PyGraphQLMessage>()?;
    m.add_class::<py_bindings::PySubscriptionExecutor>()?;
    m.add_class::<py_bindings::PyEventBusConfig>()?;
    Ok(())
}

// In fraiseql_rs() module function
subscriptions::py_bindings::init_subscriptions(m)?;
```

---

### PHASE 2: Async Event Distribution Engine (2 weeks, 30 hours)
**Goal**: Build the fast path - Rust handles all event distribution

#### 2.1 Enhanced EventBus Architecture (10 hours)

Current `EventBus` trait is good, but extend with:
```rust
#[async_trait::async_trait]
pub trait EventBus: Send + Sync {
    // Existing methods...
    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError>;

    // NEW: Direct integration with subscription executor
    async fn publish_with_executor(
        &self,
        event: Arc<Event>,
        executor: Arc<SubscriptionExecutor>,
    ) -> Result<(), SubscriptionError> {
        // 1. Publish event normally
        self.publish(event.clone()).await?;

        // 2. Dispatch to all matching subscriptions
        executor.dispatch_event_to_subscriptions(&event).await?;

        Ok(())
    }

    // NEW: Stream events to single subscription
    async fn subscribe_to_subscription(
        &self,
        subscription_id: &str,
        channels: Vec<String>,
    ) -> Result<EventStream, SubscriptionError>;
}
```

#### 2.2 Subscription Event Dispatcher (12 hours)

New `SubscriptionExecutor` enhancement:
```rust
impl SubscriptionExecutor {
    // CRITICAL METHOD: Fast event dispatch
    pub async fn dispatch_event_to_subscriptions(
        &self,
        event: &Arc<Event>,
    ) -> Result<(), SubscriptionError> {
        // 1. Find all subscriptions listening on this event's channel
        let subscriptions = self.subscriptions_by_channel(&event.channel);

        // 2. For each subscription, process in parallel
        let mut futures = vec![];
        for (sub_id, sub) in subscriptions {
            let sub_clone = sub.clone();
            let event_clone = event.clone();

            futures.push(async move {
                self.dispatch_event_to_single(sub_id, &event_clone).await
            });
        }

        // Execute all in parallel
        futures::future::join_all(futures).await;

        Ok(())
    }

    async fn dispatch_event_to_single(
        &self,
        subscription_id: &str,
        event: &Arc<Event>,
    ) -> Result<(), SubscriptionError> {
        // 1. Get subscription metadata
        let sub = self.get_subscription(subscription_id)?;

        // 2. Apply security filter (integrated 5 modules)
        let filter = SecurityAwareEventFilter::new(
            sub.base_filter.clone(),
            sub.security_context.clone(),
        );

        if !filter.should_deliver_event(event) {
            // Record rejection
            self.metrics.record_violation_x();
            return Ok(());
        }

        // 3. Apply rate limiting
        if !self.rate_limiter.allow_event(subscription_id) {
            self.metrics.record_rate_limit();
            return Ok(());
        }

        // 4. Invoke Python resolver (single blocking call)
        let resolver_result = self.invoke_python_resolver(
            &sub.resolver_fn,
            &sub.variables,
            event,
        )?;

        // 5. Encode response to pre-serialized bytes
        let response_bytes = self.encode_response_bytes(
            subscription_id,
            &sub.operation_name,
            resolver_result,
        )?;

        // 6. Queue response for WebSocket delivery
        self.queue_response(subscription_id, response_bytes)?;

        Ok(())
    }

    // Invoke Python resolver from Rust (blocking)
    fn invoke_python_resolver(
        &self,
        resolver_fn: &Py<PyAny>,  // Python function
        variables: &HashMap<String, Value>,
        event: &Arc<Event>,
    ) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            resolver_fn.call1(
                py,
                (
                    event_to_python_dict(py, event)?,
                    json_to_python_dict(py, variables)?,
                ),
            )
        })
    }

    // Encode response to bytes (RustResponseBytes pattern)
    fn encode_response_bytes(
        &self,
        subscription_id: &str,
        operation_name: &Option<String>,
        result: PyObject,
    ) -> PyResult<Vec<u8>> {
        Python::with_gil(|py| {
            // Convert Python result to JSON
            let json_value = python_to_json_value(py, &result)?;

            // Build GraphQL response
            let response = serde_json::json!({
                "type": "next",
                "id": subscription_id,
                "payload": {
                    "data": json_value
                }
            });

            // Serialize to bytes (no intermediate steps)
            Ok(serde_json::to_vec(&response)?)
        })
    }

    // Queue for WebSocket delivery (async-safe)
    fn queue_response(
        &self,
        subscription_id: &str,
        response_bytes: Vec<u8>,
    ) -> Result<(), SubscriptionError> {
        // Store in per-subscription buffer (DashMap)
        self.response_queues.entry(subscription_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(VecDeque::new())))
            .lock()
            .push(response_bytes);

        Ok(())
    }
}
```

**Key Design**:
- Event dispatch is fully parallel (no bottleneck)
- Security filtering happens once per subscription (not per message)
- Python resolver invoked once per event (acceptable overhead)
- Response pre-serialized to bytes (zero-copy to HTTP)
- Rate limiting in Rust (fast, no Python calls)

#### 2.3 Response Queue Management (8 hours)

Add to SubscriptionExecutor:
```rust
pub struct SubscriptionExecutor {
    // Existing fields...
    subscriptions: Arc<DashMap<String, SubscriptionData>>,

    // NEW: Response queues per subscription
    response_queues: Arc<DashMap<String, Arc<tokio::sync::Mutex<VecDeque<Vec<u8>>>>>>,

    // NEW: Channels for WebSocket notification
    response_notifiers: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<()>>>,
}

impl SubscriptionExecutor {
    // Python calls this in a loop to get next response
    pub fn next_response(&self, subscription_id: &str) -> Option<Vec<u8>> {
        // Non-blocking pop from queue
        self.response_queues
            .get(subscription_id)
            .and_then(|queue_ref| {
                // Try to get without blocking
                if let Ok(mut queue) = queue_ref.try_lock() {
                    queue.pop_front()
                } else {
                    None
                }
            })
    }

    // Notify WebSocket of pending response (unblock Python)
    async fn notify_response(&self, subscription_id: &str) {
        if let Some((_, notifier)) = self.response_notifiers.get(subscription_id) {
            let _ = notifier.send(());
        }
    }
}
```

---

### PHASE 3: Python High-Level API (1 week, 20 hours)
**Goal**: Simple async interface for users to write Python code only

#### 3.1 SubscriptionManager (10 hours)
```python
# src/fraiseql/subscriptions/manager.py

from fraiseql import _fraiseql_rs
import asyncio
from typing import Optional, Dict, Any, Callable

class SubscriptionManager:
    """High-level subscription manager.

    Users interact with this class. All heavy lifting happens in Rust.
    """

    def __init__(
        self,
        event_bus_config: _fraiseql_rs.PyEventBusConfig,
    ):
        """Initialize with event bus configuration."""
        self.executor = _fraiseql_rs.PySubscriptionExecutor()
        self.event_bus_config = event_bus_config
        self.subscriptions: Dict[str, 'SubscriptionData'] = {}

    async def create_subscription(
        self,
        subscription_id: str,
        connection_id: str,
        query: str,
        operation_name: Optional[str],
        variables: Dict[str, Any],
        resolver_fn: Callable,
        user_id: str,
        tenant_id: str,
    ) -> None:
        """Register a subscription in the Rust executor.

        This is the main subscription creation entry point.
        Heavy lifting (event distribution) happens in Rust.
        """
        # Store Python resolver function for later invocation
        self.subscriptions[subscription_id] = SubscriptionData(
            query=query,
            operation_name=operation_name,
            variables=variables,
            resolver_fn=resolver_fn,
            user_id=user_id,
            tenant_id=tenant_id,
        )

        # Register in Rust executor
        # (This is fast - just stores metadata)
        self.executor.register_subscription(
            connection_id=connection_id,
            subscription_id=subscription_id,
            query=query,
            operation_name=operation_name,
            variables=variables,
            user_id=user_id,
            tenant_id=tenant_id,
        )

    async def publish_event(
        self,
        event_type: str,
        channel: str,
        data: Dict[str, Any],
    ) -> None:
        """Publish event to all subscriptions.

        Rust handles:
        1. Event creation
        2. Finding subscriptions on this channel
        3. Security filtering per subscription
        4. Rate limiting per user
        5. Invoking resolvers
        6. Response serialization
        7. Queuing for WebSocket delivery
        """
        # Call Rust (blocking, but fast)
        self.executor.publish_event(
            event_type=event_type,
            channel=channel,
            data=data,
        )

    async def get_next_event(
        self,
        subscription_id: str,
    ) -> Optional[bytes]:
        """Get next event for subscription (pre-serialized bytes).

        Non-blocking call - returns immediately if event queued,
        None if queue empty.
        """
        return self.executor.next_event(subscription_id)

    async def complete_subscription(self, subscription_id: str) -> None:
        """Clean up subscription."""
        self.executor.complete_subscription(subscription_id)
        if subscription_id in self.subscriptions:
            del self.subscriptions[subscription_id]

    def get_metrics(self) -> Dict[str, Any]:
        """Get subscription metrics."""
        return self.executor.get_metrics()


class SubscriptionData:
    """Metadata stored per subscription (Python side)."""
    def __init__(self, query, operation_name, variables, resolver_fn, user_id, tenant_id):
        self.query = query
        self.operation_name = operation_name
        self.variables = variables
        self.resolver_fn = resolver_fn  # Python function
        self.user_id = user_id
        self.tenant_id = tenant_id
```

#### 3.2 FastAPI Router (10 hours)
```python
# src/fraiseql/fastapi/subscriptions.py

from fastapi import APIRouter, WebSocket, WebSocketDisconnect, Depends
import asyncio
import json
from uuid import uuid4

class SubscriptionRouterFactory:
    """Create FastAPI WebSocket router for GraphQL subscriptions."""

    @staticmethod
    def create(
        manager: SubscriptionManager,
        path: str = "/graphql/subscriptions",
        auth_handler: Optional[Callable] = None,
    ) -> APIRouter:
        """Create router with WebSocket endpoint.

        Usage:
            manager = SubscriptionManager(event_bus_config)
            router = SubscriptionRouterFactory.create(manager)
            app.include_router(router)
        """
        router = APIRouter()

        @router.websocket(path)
        async def websocket_endpoint(websocket: WebSocket):
            """Handle GraphQL subscription connections.

            Protocol: graphql-transport-ws
            """
            await websocket.accept(subprotocol="graphql-transport-ws")
            connection_id = str(uuid4())
            active_subscriptions: Dict[str, str] = {}  # sub_id → channel

            try:
                while True:
                    # Receive message from client
                    data = await websocket.receive_json()
                    msg_type = data.get("type")

                    if msg_type == "connection_init":
                        # Authentication (optional)
                        auth_data = data.get("payload", {})
                        if auth_handler:
                            user_context = await auth_handler(auth_data)
                        else:
                            user_context = {"user_id": "anonymous"}

                        # Send ack
                        await websocket.send_json({
                            "type": "connection_ack",
                        })

                    elif msg_type == "subscribe":
                        # Create subscription
                        sub_id = data.get("id")
                        payload = data.get("payload", {})

                        try:
                            # Register subscription in Rust executor
                            await manager.create_subscription(
                                subscription_id=sub_id,
                                connection_id=connection_id,
                                query=payload.get("query"),
                                operation_name=payload.get("operationName"),
                                variables=payload.get("variables", {}),
                                resolver_fn=get_resolver_for_query(payload.get("query")),
                                user_id=user_context.get("user_id"),
                                tenant_id=user_context.get("tenant_id", ""),
                            )

                            active_subscriptions[sub_id] = payload.get("query")

                            # Start event listener task for this subscription
                            asyncio.create_task(
                                listen_for_events(
                                    websocket, manager, sub_id, connection_id
                                )
                            )

                        except Exception as e:
                            await websocket.send_json({
                                "type": "error",
                                "id": sub_id,
                                "payload": [{"message": str(e)}],
                            })

                    elif msg_type == "complete":
                        sub_id = data.get("id")
                        await manager.complete_subscription(sub_id)
                        if sub_id in active_subscriptions:
                            del active_subscriptions[sub_id]

                        await websocket.send_json({
                            "type": "complete",
                            "id": sub_id,
                        })

                    elif msg_type == "ping":
                        # Keep-alive
                        await websocket.send_json({"type": "pong"})

            except WebSocketDisconnect:
                # Clean up on disconnect
                for sub_id in active_subscriptions.keys():
                    await manager.complete_subscription(sub_id)

        return router

        async def listen_for_events(
            websocket: WebSocket,
            manager: SubscriptionManager,
            subscription_id: str,
            connection_id: str,
        ) -> None:
            """Listen for events on subscription and send to client.

            This runs in background task per subscription.
            Rust queues responses, Python just sends them.
            """
            while True:
                try:
                    # Get next event (non-blocking)
                    response_bytes = await manager.get_next_event(subscription_id)

                    if response_bytes:
                        # Send pre-serialized bytes directly
                        await websocket.send_bytes(response_bytes)
                    else:
                        # Small sleep to avoid busy-waiting
                        await asyncio.sleep(0.001)

                except Exception as e:
                    # Send error and exit
                    await websocket.send_json({
                        "type": "error",
                        "id": subscription_id,
                        "payload": [{"message": str(e)}],
                    })
                    break
```

---

### PHASE 4: Integration & Testing (2 weeks, 30 hours)

#### 4.1 Test Suite Structure (15 hours)

```python
# tests/test_subscriptions_e2e.py

@pytest.mark.asyncio
async def test_subscription_full_workflow():
    """Complete subscription workflow test."""
    # 1. Setup
    event_bus_config = _fraiseql_rs.PyEventBusConfig.memory()
    manager = SubscriptionManager(event_bus_config)

    # 2. Create subscription
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { users { id name } }",
        operation_name="OnUserUpdated",
        variables={},
        resolver_fn=mock_resolver,
        user_id="user1",
        tenant_id="tenant1",
    )

    # 3. Publish event
    await manager.publish_event(
        event_type="userCreated",
        channel="users",
        data={"id": "123", "name": "Alice"},
    )

    # 4. Get response (pre-serialized bytes)
    response_bytes = await manager.get_next_event("sub1")
    assert response_bytes is not None

    # 5. Parse and verify
    response = json.loads(response_bytes)
    assert response["type"] == "next"
    assert response["id"] == "sub1"

    # 6. Cleanup
    await manager.complete_subscription("sub1")
```

#### 4.2 Performance Benchmarks (10 hours)

```python
# tests/test_subscriptions_performance.py

@pytest.mark.asyncio
async def test_event_distribution_throughput():
    """Benchmark event distribution throughput."""
    manager = SubscriptionManager(...)

    # Create 100 subscriptions
    for i in range(100):
        await manager.create_subscription(...)

    # Publish 10,000 events and measure time
    start = time.time()
    for i in range(10_000):
        await manager.publish_event(
            event_type="test",
            channel="test",
            data={"id": i},
        )
    elapsed = time.time() - start

    # Target: <1ms per event with 100 subscriptions
    assert elapsed < 10.0  # 10 seconds for 10k events = 1ms per event

@pytest.mark.asyncio
async def test_security_filtering_overhead():
    """Measure security filtering performance."""
    # Create subscriptions with different security contexts
    # Publish events and verify filtering
    # Measure overhead of security validation
```

#### 4.3 Compilation & Type Checking (5 hours)

```bash
# Verify Rust code compiles
cargo build --lib 2>&1

# Verify Python code is type-safe
mypy src/fraiseql/subscriptions/manager.py
mypy src/fraiseql/fastapi/subscriptions.py
```

---

### PHASE 5: Documentation & Examples (1 week, 20 hours)

#### 5.1 User Guide (10 hours)

Create `docs/subscriptions-guide.md`:

```markdown
# GraphQL Subscriptions - User Guide

## Quick Start

```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# 1. Create manager with event bus
event_bus_config = _fraiseql_rs.PyEventBusConfig.redis(
    url="redis://localhost:6379",
    consumer_group="my-app",
)
manager = SubscriptionManager(event_bus_config)

# 2. Create FastAPI app with subscription support
from fraiseql.fastapi.subscriptions import SubscriptionRouterFactory
router = SubscriptionRouterFactory.create(manager)
app.include_router(router)

# 3. Define resolver (user writes Python only!)
async def resolve_user_updated(event_data: dict, variables: dict) -> dict:
    """User writes resolver to transform event to subscription response."""
    return {
        "user": {
            "id": event_data["id"],
            "name": event_data["name"],
        }
    }

# 4. Publish event (framework handles distribution)
await manager.publish_event(
    event_type="userUpdated",
    channel="users",
    data={"id": "123", "name": "Alice"},
)
```

## Architecture

[Describe data flow, where Rust handles what, performance characteristics]

## Performance

- Event publishing: <1ms
- Event delivery per subscriber: <1ms
- Total E2E latency: <10ms (database update → client message)
- 10,000+ concurrent subscriptions per instance
- Throughput: 10,000+ events/second

## Security

- Row-level filtering
- Tenant isolation (multi-tenant SaaS)
- RBAC field-level access control
- Federation boundary enforcement
- Variable scope validation
```

#### 5.2 API Reference (5 hours)

Document:
- `SubscriptionManager` class
- `SubscriptionRouterFactory` class
- `PyEventBusConfig` options
- `PySubscriptionExecutor` (internal)
- Error handling

#### 5.3 Example Application (5 hours)

Create `examples/subscriptions_app.py`:
- Complete working FastAPI app
- Multiple subscription types
- Error handling
- Metrics/monitoring

---

## Summary Table

| Phase | Component | Hours | Lines | Key Achievement |
|-------|-----------|-------|-------|-----------------|
| 1 | PyO3 Bindings | 30 | ~1200 | Minimal Python-Rust interface |
| 2 | Event Distribution | 30 | ~1500 | Async Rust engine with event dispatch |
| 3 | Python API | 20 | ~600 | High-level user interface |
| 4 | Testing | 30 | ~1000 | Performance validated |
| 5 | Documentation | 20 | ~800 | Complete examples & guides |
| **TOTAL** | | **130** | **~5100** | **Fast, Rust-heavy subscriptions** |

---

## Key Design Decisions

### 1. ✅ Sync Rust Functions, Async Internals
- Python calls synchronous FFI functions
- Rust uses `block_on()` internally (established pattern)
- No pyo3-asyncio overhead
- No Python coroutine complexity

### 2. ✅ Pre-Serialized Responses
- Rust outputs `Vec<u8>` (JSON bytes)
- Python sends bytes directly (no parsing)
- Follows RustResponseBytes pattern
- Zero intermediate conversions

### 3. ✅ Event Dispatch in Rust
- All subscription matching in Rust
- All security filtering in Rust
- All rate limiting in Rust
- Python only sends bytes to client

### 4. ✅ Single Executor Pattern
- One `PySubscriptionExecutor` instance
- Manages all subscriptions across all connections
- Thread-safe via Arc<DashMap>
- Shared global runtime

### 5. ✅ Resolver Invocation (Only Python Call)
- Python resolver invoked once per event per subscription
- Result is converted to JSON and pre-serialized
- This is acceptable Python overhead (one call per relevant event)
- Everything else happens in Rust

---

## Performance Characteristics (Estimated)

### E2E Latency (database event → subscription message)
- Database transaction commits: ~1ms
- Event creation in Rust: <0.1ms
- Event bus publish: <1ms
- Subscription matching: <0.5ms per subscription
- Security filtering: <1ms per subscription
- Python resolver invocation: ~5ms per subscription
- Response serialization: <0.5ms
- WebSocket send: <1ms
- **Total: <10ms** (with reasonable resolver complexity)

### Throughput
- Single instance: 10,000+ events/sec
- Per subscription: 1,000+ events/sec
- With 100 subscriptions: 100,000+ events/sec total

### Scalability
- Concurrent subscriptions: Limited by memory (~100-1000 per GB)
- Event bus throughput: Redis can handle millions/sec
- Security validation: O(1) per subscription
- No global locks (DashMap is lock-free)

---

## Critical Implementation Notes

### Note 1: Python Resolver Invocation
```rust
// Single performance-critical call
fn invoke_python_resolver(
    resolver_fn: &Py<PyAny>,
    variables: &HashMap<String, Value>,
    event: &Arc<Event>,
) -> PyResult<PyObject> {
    Python::with_gil(|py| {
        resolver_fn.call1(py, (event, variables))
    })
}
```

This is the only Python code invoked per event. Everything else is Rust.

### Note 2: Response Pre-Serialization
```rust
// Returns bytes, not dict
fn encode_response_bytes(
    subscription_id: &str,
    result: PyObject,
) -> PyResult<Vec<u8>> {
    let json = python_to_json_value(result)?;
    serde_json::to_vec(&json)  // bytes, not dict
}
```

Python never sees JSON as dict. Saves parse/serialize cycle.

### Note 3: Global Tokio Runtime
```rust
// Use existing global runtime (already initialized)
let runtime = Arc::new(crate::db::runtime::runtime().clone());
```

Don't create new runtime. Reuse existing one for consistency.

### Note 4: Non-Blocking Queue
```rust
// next_response() uses try_lock() - never blocks Python
pub fn next_response(&self, subscription_id: &str) -> Option<Vec<u8>> {
    if let Ok(mut queue) = queue_ref.try_lock() {
        queue.pop_front()
    } else {
        None  // Never block Python thread
    }
}
```

Python uses small sleep if queue empty, never waits for lock.

---

## Success Criteria

### Performance
- [ ] Event publishing: <1ms
- [ ] Event delivery per subscription: <1ms
- [ ] E2E latency: <10ms
- [ ] 10,000+ concurrent subscriptions per instance
- [ ] Throughput: >10,000 events/sec

### Functionality
- [ ] All 5 security modules enforced
- [ ] Multi-tenant isolation working
- [ ] RBAC field-level access control
- [ ] Rate limiting per user/subscription
- [ ] Metrics and monitoring

### Usability
- [ ] Users write Python code only
- [ ] Simple `@subscription` decorator
- [ ] Clear resolver function pattern
- [ ] Complete examples and docs

### Code Quality
- [ ] Zero compiler errors
- [ ] Zero clippy warnings
- [ ] 30+ integration tests
- [ ] 50+ performance benchmarks
- [ ] <5000 lines total (Rust + Python)

---

## Risk Mitigation

| Risk | Severity | Mitigation |
|------|----------|-----------|
| Python resolver performance | Medium | Optimize with numba/Cython if needed |
| Event bus bottleneck | Low | Redis can handle millions/sec |
| Memory with 10k+ subscriptions | Medium | Monitor memory, shard if needed |
| GIL contention | Low | Minimal Python code in hot path |
| Tokio runtime issues | Low | Reuse existing, proven pattern |

---

## Timeline

- Week 1: PyO3 bindings (Phase 1)
- Week 2: Event distribution engine (Phase 2)
- Week 3: Python API and testing (Phases 3-4)
- Week 4: Documentation and polish (Phase 5)

**Total: 4 weeks, 130 hours (solid week of work)**

---

## Comparison: This vs Original Plan

| Aspect | Original Plan | V2 (Performance-First) |
|--------|---------------|----------------------|
| **Philosophy** | Balanced | Rust-heavy |
| **Python LOC** | ~600 | ~400 |
| **Rust LOC** | ~1200 | ~2000+ |
| **Resolver Calls** | Once per event | Once per event ✓ Same |
| **Response Format** | Dict | Pre-serialized bytes ✓ Better |
| **Security in Rust** | Yes | Yes ✓ Same |
| **Event Distribution** | Partially | Fully in Rust ✓ Better |
| **Performance** | Not specified | <10ms E2E ✓ Better |
| **Code Reuse** | Moderate | Maximum ✓ Better |
| **Complexity** | Higher | Lower ✓ Better |
| **User API** | Moderate | Minimal ✓ Better |

---

## Files to Create/Modify

### Rust
- `fraiseql_rs/src/subscriptions/py_bindings.rs` (NEW, ~1200 lines)
- `fraiseql_rs/src/subscriptions/executor.rs` (ENHANCE, +500 lines)
- `fraiseql_rs/src/subscriptions/event_dispatcher.rs` (NEW, ~800 lines)
- `fraiseql_rs/src/lib.rs` (MODIFY, +20 lines)

### Python
- `src/fraiseql/subscriptions/manager.py` (NEW, ~300 lines)
- `src/fraiseql/fastapi/subscriptions.py` (NEW, ~300 lines)
- `src/fraiseql/subscriptions/__init__.py` (MODIFY, +5 lines)

### Documentation
- `docs/subscriptions-guide.md` (NEW, ~300 lines)
- `docs/subscriptions-api.md` (NEW, ~200 lines)
- `examples/subscriptions_app.py` (NEW, ~200 lines)

### Tests
- `tests/test_subscriptions_e2e.py` (NEW, ~500 lines)
- `tests/test_subscriptions_perf.py` (NEW, ~500 lines)
- `fraiseql_rs/src/subscriptions/py_bindings_tests.rs` (NEW, ~300 lines)

**Total New: ~5100 lines across Rust, Python, and Docs**
