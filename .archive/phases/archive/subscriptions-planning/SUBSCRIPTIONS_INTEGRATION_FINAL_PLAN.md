# GraphQL Subscriptions Python Integration - FINAL PLAN

**Date**: January 3, 2026
**Status**: Ready for Implementation
**Version**: 3.0 (Integrated V2 + HTTP Abstraction)
**Timeline**: 4 weeks / 130 hours
**Philosophy**: Maximum Rust, Minimal Python, Users write only business logic in Python

---

## 🎯 Executive Summary

This plan integrates GraphQL subscriptions into FraiseQL's Python framework with the following design principles:

1. **Everything fast happens in Rust** - Event distribution, security, filtering, rate limiting
2. **Python for user business logic only** - Resolvers, connection setup, configuration
3. **Pluggable HTTP server abstraction** - Works with FastAPI, Starlette, Rust server (future), or custom
4. **Zero-copy data movement** - Arc-based events, pre-serialized responses
5. **Performance target**: <10ms end-to-end (database event → subscription message)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ USER CODE (Python)                                              │
│ ├─ @subscription decorator                                      │
│ ├─ async def resolver(event: dict, variables: dict) -> dict     │
│ └─ Defines: query, operation_name, channels                     │
└────────────────────────┬────────────────────────────────────────┘
                         │ (Registration only)
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│ RUST SUBSCRIPTION ENGINE (Minimal Python interaction)           │
│ ┌───────────────────────────────────────────────────────────┐   │
│ │ Subscription Registry (DashMap)                           │   │
│ │ ├─ subscription_id → SubscriptionMetadata               │   │
│ │ ├─ connection_id → active subscriptions                  │   │
│ │ └─ Per-subscription response queues                      │   │
│ └───────────────────────────────────────────────────────────┘   │
│                         ↓                                        │
│ ┌───────────────────────────────────────────────────────────┐   │
│ │ Event Bus (Async Core)                                   │   │
│ │ ├─ Redis backend (production)                            │   │
│ │ ├─ PostgreSQL backend (fallback)                         │   │
│ │ └─ InMemory backend (testing)                            │   │
│ └───────────────────────────────────────────────────────────┘   │
│                         ↓                                        │
│ ┌───────────────────────────────────────────────────────────┐   │
│ │ Subscription Event Dispatcher                            │   │
│ │ ├─ Find subscriptions by channel (parallel)              │   │
│ │ ├─ Apply SecurityAwareEventFilter (5 modules)            │   │
│ │ ├─ Apply RateLimiter per user                            │   │
│ │ ├─ [ONE] Invoke Python resolver (blocking call)          │   │
│ │ ├─ Encode response to pre-serialized bytes               │   │
│ │ └─ Queue for WebSocket delivery                          │   │
│ └───────────────────────────────────────────────────────────┘   │
│                         ↓ (pre-serialized bytes)                │
│ ┌───────────────────────────────────────────────────────────┐   │
│ │ Response Queues (lock-free, per subscription)             │   │
│ └───────────────────────────────────────────────────────────┘   │
└────────────────────────┬────────────────────────────────────────┘
                         │ (bytes only)
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│ HTTP ABSTRACTION LAYER (Framework-agnostic)                     │
│ ├─ WebSocketAdapter interface                                   │
│ ├─ SubscriptionProtocolHandler interface                        │
│ │  └─ GraphQLTransportWSHandler (implements graphql-transport-ws)│
│ └─ Framework implementations:                                    │
│    ├─ FastAPIWebSocketAdapter + FastAPI router                  │
│    ├─ StarletteWebSocketAdapter + Starlette handler             │
│    └─ CustomServerAdapter template                              │
└────────────────────────┬────────────────────────────────────────┘
                         │ (sends bytes directly to client)
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│ HTTP CLIENT (WebSocket)                                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Phases

### PHASE 1: PyO3 Core Bindings (2 weeks, 30 hours)

**Objective**: Expose Rust engine to Python with minimal overhead

#### 1.1 Subscription Payload Types (6 hours)

**File**: `fraiseql_rs/src/subscriptions/py_bindings.rs`

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

#[pyclass]
pub struct PyGraphQLMessage {
    #[pyo3(get)]
    pub type_: String,
    #[pyo3(get)]
    pub id: Option<String>,
    #[pyo3(get)]
    pub payload: Option<Py<PyDict>>,
}
```

**Strategy**: Minimal type stubs for passing data between Python and Rust.

#### 1.2 Core Subscription Executor (8 hours)

```rust
#[pyclass]
pub struct PySubscriptionExecutor {
    executor: Arc<SubscriptionExecutor>,
    runtime: Arc<tokio::runtime::Runtime>,
}

#[pymethods]
impl PySubscriptionExecutor {
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
        // Fast: just stores in DashMap
    }

    pub fn publish_event(
        &self,
        event_type: String,
        channel: String,
        data: &Bound<PyDict>,
    ) -> PyResult<()> {
        // Async via block_on(), uses global tokio runtime
    }

    pub fn next_event(
        &self,
        subscription_id: String,
    ) -> PyResult<Option<Vec<u8>>> {
        // Returns pre-serialized bytes
    }

    pub fn complete_subscription(&self, subscription_id: String) -> PyResult<()> {
        // Cleanup
    }

    pub fn get_metrics(&self) -> Py<PyDict> {
        // Return metrics as Python dict
    }
}
```

**Key Design**:
- `register_subscription()` is O(1) - just stores metadata
- `publish_event()` does async work via `block_on()`
- `next_event()` returns pre-serialized bytes (critical for performance)
- Uses global tokio runtime (already initialized in `db::runtime`)

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
```

**Strategy**: Wrap EventBusConfig, don't expose EventBus directly to Python.

#### 1.4 Module Registration (5 hours)

Update `fraiseql_rs/src/lib.rs`:

```rust
pub fn init_subscriptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<py_bindings::PySubscriptionPayload>()?;
    m.add_class::<py_bindings::PyGraphQLMessage>()?;
    m.add_class::<py_bindings::PySubscriptionExecutor>()?;
    m.add_class::<py_bindings::PyEventBusConfig>()?;
    Ok(())
}
```

---

### PHASE 2: Async Event Distribution Engine (2 weeks, 30 hours)

**Objective**: Build the fast path - Rust handles all event distribution

#### 2.1 Enhanced EventBus Architecture (10 hours)

Extend existing EventBus trait:

```rust
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: Arc<Event>) -> Result<(), SubscriptionError>;

    // NEW: Direct integration with subscription executor
    async fn publish_with_executor(
        &self,
        event: Arc<Event>,
        executor: Arc<SubscriptionExecutor>,
    ) -> Result<(), SubscriptionError> {
        self.publish(event.clone()).await?;
        executor.dispatch_event_to_subscriptions(&event).await?;
        Ok(())
    }
}
```

#### 2.2 Subscription Event Dispatcher (12 hours)

**Critical method** in SubscriptionExecutor:

```rust
pub async fn dispatch_event_to_subscriptions(
    &self,
    event: &Arc<Event>,
) -> Result<(), SubscriptionError> {
    // 1. Find all subscriptions listening on this channel
    let subscriptions = self.subscriptions_by_channel(&event.channel);

    // 2. Process in parallel
    let mut futures = vec![];
    for (sub_id, sub) in subscriptions {
        futures.push(async move {
            self.dispatch_event_to_single(sub_id, &event.clone()).await
        });
    }

    futures::future::join_all(futures).await;
    Ok(())
}

async fn dispatch_event_to_single(
    &self,
    subscription_id: &str,
    event: &Arc<Event>,
) -> Result<(), SubscriptionError> {
    // 1. Get subscription metadata
    // 2. Apply SecurityAwareEventFilter (5 modules integrated)
    // 3. Apply RateLimiter
    // 4. Invoke Python resolver (ONE blocking call)
    // 5. Encode response to pre-serialized bytes
    // 6. Queue for WebSocket delivery
    Ok(())
}

fn invoke_python_resolver(
    &self,
    resolver_fn: &Py<PyAny>,
    variables: &HashMap<String, Value>,
    event: &Arc<Event>,
) -> PyResult<PyObject> {
    Python::with_gil(|py| {
        resolver_fn.call1(
            py,
            (event_to_python_dict(py, event)?, json_to_python_dict(py, variables)?),
        )
    })
}

fn encode_response_bytes(
    &self,
    subscription_id: &str,
    operation_name: &Option<String>,
    result: PyObject,
) -> PyResult<Vec<u8>> {
    Python::with_gil(|py| {
        let json_value = python_to_json_value(py, &result)?;
        let response = serde_json::json!({
            "type": "next",
            "id": subscription_id,
            "payload": { "data": json_value }
        });
        Ok(serde_json::to_vec(&response)?)
    })
}
```

**Key Performance Decisions**:
- ✅ Event dispatch fully parallel (no bottleneck)
- ✅ Security filtering happens once per subscription (not per message)
- ✅ Python resolver invoked once per event (single blocking call per distribution)
- ✅ Response pre-serialized to bytes (zero-copy to HTTP)
- ✅ Rate limiting in Rust (fast, no Python calls)

#### 2.3 Response Queue Management (8 hours)

Add to SubscriptionExecutor:

```rust
pub struct SubscriptionExecutor {
    subscriptions: Arc<DashMap<String, SubscriptionData>>,
    response_queues: Arc<DashMap<String, Arc<tokio::sync::Mutex<VecDeque<Vec<u8>>>>>>,
    response_notifiers: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<()>>>,
}

impl SubscriptionExecutor {
    pub fn next_response(&self, subscription_id: &str) -> Option<Vec<u8>> {
        // Non-blocking pop from queue
    }

    async fn notify_response(&self, subscription_id: &str) {
        // Notify WebSocket of pending response
    }
}
```

---

### PHASE 3: Python High-Level API (3 weeks, 30 hours)

**Objective**: Simple async interface, framework-agnostic

#### 3.0 HTTP Abstraction Layer (10 hours, NEW)

**File**: `src/fraiseql/subscriptions/http_adapter.py`

Provides framework-agnostic interfaces:

```python
class WebSocketAdapter(ABC):
    """Abstract WebSocket interface - implement by each HTTP framework."""
    @abstractmethod
    async def accept(self, subprotocol: Optional[str] = None) -> None: ...
    @abstractmethod
    async def receive_json(self) -> Dict[str, Any]: ...
    @abstractmethod
    async def send_json(self, data: Dict[str, Any]) -> None: ...
    @abstractmethod
    async def send_bytes(self, data: bytes) -> None: ...  # Critical for performance
    @abstractmethod
    async def close(self, code: int = 1000, reason: str = "") -> None: ...
    @property
    @abstractmethod
    def is_connected(self) -> bool: ...
```

Implementations:
- `FastAPIWebSocketAdapter` - Wraps FastAPI WebSocket
- `StarletteWebSocketAdapter` - Wraps Starlette WebSocket
- `CustomServerAdapter` - Template for custom frameworks

Protocol handler:

```python
class SubscriptionProtocolHandler(ABC):
    @abstractmethod
    async def handle_connection(
        self,
        websocket: WebSocketAdapter,
        manager: "SubscriptionManager",
        auth_handler: Optional[Callable] = None,
    ) -> None: ...

class GraphQLTransportWSHandler(SubscriptionProtocolHandler):
    """Implements graphql-transport-ws protocol (framework-agnostic)."""
    # Handles: connection_init, subscribe, next, error, complete, ping/pong
```

**Benefits**:
- ✅ Zero framework-specific code in core
- ✅ Easy to add Rust HTTP server later (just implement adapter)
- ✅ Support multiple protocols (graphql-ws, graphql-transport-ws, custom)
- ✅ Testable without real framework

#### 3.1 Framework-Agnostic SubscriptionManager (8 hours)

**File**: `src/fraiseql/subscriptions/manager.py`

```python
class SubscriptionManager:
    """Works with any HTTP framework via adapter pattern."""

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
        """Register subscription in Rust executor."""

    async def publish_event(
        self,
        event_type: str,
        channel: str,
        data: Dict[str, Any],
    ) -> None:
        """Publish event to Rust executor."""

    async def get_next_event(
        self,
        subscription_id: str,
    ) -> Optional[bytes]:
        """Get next pre-serialized event bytes."""

    async def complete_subscription(self, subscription_id: str) -> None:
        """Clean up subscription."""

    def get_metrics(self) -> Dict[str, Any]:
        """Get subscription metrics."""
```

**Design**: Zero framework-specific code. All heavy lifting done in Rust.

#### 3.2 Framework-Specific Integrations (12 hours)

##### FastAPI Integration (4 hours)

**File**: `src/fraiseql/integrations/fastapi_subscriptions.py`

```python
class SubscriptionRouterFactory:
    @staticmethod
    def create(
        manager: SubscriptionManager,
        path: str = "/graphql/subscriptions",
        auth_handler: Optional[Callable] = None,
    ) -> APIRouter:
        """Create FastAPI router.

        Usage:
            manager = SubscriptionManager(config)
            router = SubscriptionRouterFactory.create(manager)
            app.include_router(router)
        """
        router = APIRouter()
        handler = GraphQLTransportWSHandler()

        @router.websocket(path)
        async def websocket_endpoint(websocket: WebSocket):
            adapter = FastAPIWebSocketAdapter(websocket)
            await handler.handle_connection(adapter, manager, auth_handler)

        return router
```

##### Starlette Integration (4 hours)

**File**: `src/fraiseql/integrations/starlette_subscriptions.py`

```python
def create_subscription_app(
    app: Starlette,
    manager: SubscriptionManager,
    path: str = "/graphql/subscriptions",
    auth_handler: Optional[Callable] = None,
) -> None:
    """Add subscription endpoint to Starlette app.

    Usage:
        app = Starlette()
        create_subscription_app(app, manager)
    """
    handler = GraphQLTransportWSHandler()

    async def ws_endpoint(websocket):
        adapter = StarletteWebSocketAdapter(websocket)
        await handler.handle_connection(adapter, manager, auth_handler)

    route = WebSocketRoute(path, endpoint=ws_endpoint)
    app.routes.append(route)
```

##### Custom Server Examples (4 hours)

**File**: `src/fraiseql/subscriptions/custom_server_example.py`

Template showing how to implement WebSocketAdapter for any custom HTTP framework.

---

### PHASE 4: Integration & Testing (2 weeks, 30 hours)

#### 4.1 Test Suite (15 hours)

```python
# tests/test_subscriptions_e2e.py

@pytest.mark.asyncio
async def test_subscription_full_workflow():
    """Complete subscription workflow."""
    # 1. Create manager
    config = _fraiseql_rs.PyEventBusConfig.memory()
    manager = SubscriptionManager(config)

    # 2. Create subscription
    await manager.create_subscription(...)

    # 3. Publish event
    await manager.publish_event(...)

    # 4. Get response (pre-serialized bytes)
    response_bytes = await manager.get_next_event("sub1")
    assert response_bytes is not None

    # 5. Parse and verify
    response = json.loads(response_bytes)
    assert response["type"] == "next"

@pytest.mark.asyncio
async def test_security_filtering():
    """Test security filtering integration."""
    # Verify SecurityAwareEventFilter works end-to-end

@pytest.mark.asyncio
async def test_rate_limiting():
    """Test rate limiter enforcement."""

@pytest.mark.asyncio
async def test_multi_subscription_concurrent():
    """Test 100+ concurrent subscriptions."""

@pytest.mark.asyncio
async def test_http_adapter_abstraction():
    """Test WebSocketAdapter abstraction with mocks."""
```

#### 4.2 Performance Benchmarks (10 hours)

```python
@pytest.mark.asyncio
async def test_event_distribution_throughput():
    """Benchmark: 10,000 events with 100 subscriptions.
    Target: <1ms per event (10 seconds total)
    """

@pytest.mark.asyncio
async def test_security_filtering_overhead():
    """Measure overhead of 5 security modules."""

@pytest.mark.asyncio
async def test_python_resolver_invocation_cost():
    """Measure cost of blocking Python resolver call."""

@pytest.mark.asyncio
async def test_response_serialization_throughput():
    """Measure pre-serialization performance."""
```

#### 4.3 Compilation & Type Checking (5 hours)

```bash
cargo build --lib  # Verify Rust code compiles
mypy src/fraiseql/subscriptions/  # Type-safe Python
pytest tests/  # Full test suite
```

---

### PHASE 5: Documentation & Examples (1 week, 20 hours)

#### 5.1 User Guide (10 hours)

Create `docs/subscriptions-guide.md`:

```markdown
# GraphQL Subscriptions - User Guide

## Quick Start

# With FastAPI
from fraiseql.subscriptions import SubscriptionManager
from fraiseql.integrations.fastapi_subscriptions import SubscriptionRouterFactory
from fraiseql import _fraiseql_rs

event_bus_config = _fraiseql_rs.PyEventBusConfig.redis(...)
manager = SubscriptionManager(event_bus_config)
router = SubscriptionRouterFactory.create(manager)
app.include_router(router)

# Or with Starlette
from fraiseql.integrations.starlette_subscriptions import create_subscription_app
create_subscription_app(app, manager)

# Define resolver (user writes Python!)
async def resolve_user_updated(event_data: dict, variables: dict) -> dict:
    return {"user": {"id": event_data["id"], "name": event_data["name"]}}

# Publish events
await manager.publish_event("userUpdated", "users", {"id": "123", "name": "Alice"})
```

#### 5.2 API Reference (5 hours)

Document all public classes and methods:
- `SubscriptionManager` - Main user-facing class
- `PySubscriptionExecutor` - Rust bindings
- `WebSocketAdapter` - Framework integration interface
- `GraphQLTransportWSHandler` - Protocol handler

#### 5.3 Framework Integration Examples (5 hours)

Complete working examples:
- FastAPI with authentication
- Starlette with custom middleware
- Custom HTTP server adapter template
- Redis vs PostgreSQL event bus comparison

---

## Performance Targets

| Metric | Target | Why |
|--------|--------|-----|
| **Event → Subscription** | <10ms E2E | Database event to subscription message delivery |
| **Security Filtering** | <1μs per check | 5 modules × 4-step validation |
| **Python Resolver Call** | <100μs per call | Single blocking invocation per event |
| **Response Serialization** | <10μs | Pre-serialized to bytes |
| **Throughput** | >10k events/sec | 100+ concurrent subscriptions |
| **Concurrent Subscriptions** | 10,000+ | With <100ms response latency |

**Total E2E Budget**:
- Event dispatch in Rust: <1ms
- Python resolver: <100μs
- Response queue: <1μs
- WebSocket send: <8ms (network bound)
- **Total: <10ms ✅**

---

## File Structure Created

```
fraiseql_rs/
└── src/subscriptions/
    ├── py_bindings.rs (NEW - ~500 lines)
    ├── executor.rs (EXISTING - extend ~200 lines)
    ├── event_filter.rs (EXISTING - extend ~100 lines)
    └── metrics.rs (EXISTING - extend ~50 lines)

src/fraiseql/
├── subscriptions/ (NEW directory)
│   ├── __init__.py
│   ├── manager.py (~300 lines)
│   ├── http_adapter.py (~400 lines)
│   └── custom_server_example.py (~80 lines)
└── integrations/ (NEW directory)
    ├── __init__.py
    ├── fastapi_subscriptions.py (~150 lines)
    └── starlette_subscriptions.py (~150 lines)

tests/
├── test_subscriptions_e2e.py (~300 lines)
├── test_subscriptions_performance.py (~200 lines)
└── test_subscriptions_fastapi.py (~200 lines)

docs/
└── subscriptions-guide.md (~400 lines)
```

**Total New Code**:
- Rust: ~850 lines
- Python: ~1,080 lines
- Tests: ~700 lines
- Docs: ~400 lines
- **Total: ~3,030 lines**

---

## Success Criteria

### Phase 1 ✅
- [ ] PySubscriptionExecutor compiles and tests pass
- [ ] Can call from Python: `executor.register_subscription(...)`
- [ ] Can call from Python: `executor.publish_event(...)`
- [ ] Can get responses: `executor.next_event(...)` returns bytes

### Phase 2 ✅
- [ ] Event dispatcher runs async code correctly
- [ ] Python resolver invoked once per event
- [ ] Pre-serialized responses in queue
- [ ] Performance: <1ms per event with 100 subscriptions

### Phase 3 ✅
- [ ] SubscriptionManager zero framework dependencies
- [ ] FastAPI router works (4+ tests passing)
- [ ] Starlette integration works (4+ tests passing)
- [ ] Custom server adapter example complete

### Phase 4 ✅
- [ ] E2E tests pass (security, filtering, rate limiting)
- [ ] Performance benchmarks met (>10k events/sec)
- [ ] 100+ concurrent subscriptions stable
- [ ] Type checking passes (mypy clean)

### Phase 5 ✅
- [ ] User guide complete and clear
- [ ] API reference complete
- [ ] Framework integration examples work
- [ ] README updated with subscription support

---

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Python resolver blocking | Medium | Async dispatch (other subscriptions unblocked), measure <100μs |
| GIL contention with many resolvers | High | One resolver per event, use asyncio for client reception |
| Security context cloning overhead | Low | Arc<SecurityContext> (pointer copy, not data copy) |
| WebSocket framework differences | Medium | WebSocketAdapter abstraction + tests |
| Event bus backend latency | High | Configurable (Redis, PostgreSQL, InMemory) |

---

## Comparison: Before vs After

### Before (No Subscriptions)
```
Query → Rust Pipeline → Response (fast)
```

### After (With Subscriptions)
```
Subscribe → Rust Registry
Event → Rust Dispatcher → Filter → Rate Limit → Python Resolver
       → Pre-serialize → Queue → HTTP → Client
```

**Key**: Everything except Python resolver in Rust. Python resolver called once per event (acceptable).

---

## Ready for Implementation

This plan:
- ✅ Addresses all 3 critical gaps from initial review
- ✅ Implements HTTP server abstraction for future Rust server
- ✅ Leverages proven patterns (global runtime, RustResponseBytes, Arc-based events)
- ✅ Maintains <10ms E2E performance target
- ✅ Requires 4 weeks / 130 hours
- ✅ Creates 3,030 lines of code (750 lines per week)
- ✅ 100% framework-agnostic Python core

**Next Step**: Begin Phase 1 implementation - Create `fraiseql_rs/src/subscriptions/py_bindings.rs`
