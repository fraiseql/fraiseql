# Plan V3 Changes Summary - HTTP Server Abstraction

**Date**: January 3, 2026
**Triggered By**: User requirement: "end goal includes dropping FastAPI for 'choose your HTTP server'"
**Impact**: Architectural change to Phase 3, maintains 4-week timeline

---

## What Changed

### V2 → V3 Key Differences

| Aspect | V2 | V3 |
|--------|----|----|
| **HTTP Framework** | FastAPI hardcoded | Abstracted via adapter |
| **Phase 3 Structure** | 20 hours (2 components) | 30 hours (3 components) |
| **FastAPI Code** | In SubscriptionManager | In fastapi_subscriptions.py |
| **Starlette Support** | None (would require rewrite) | Built-in via adapter |
| **Custom Servers** | Would require rewrite | Template provided |
| **Rust HTTP Server** | Complex to add later | Just implement one adapter |
| **Protocol Logic** | Scattered in router | Centralized in handler |
| **Testing** | Framework-dependent | Framework-independent |

---

## Architecture Change

### Before (V2)
```
SubscriptionManager
    └─ FastAPI-specific WebSocket code
        ├─ connection_init handling
        ├─ subscribe handling
        └─ listen_for_events()
```

**Problem**: FastAPI knowledge bleeding into core manager

### After (V3)
```
SubscriptionManager
    ├─ Framework-agnostic core
    └─ Uses WebSocketAdapter abstraction
        ├─ FastAPI adapter
        ├─ Starlette adapter
        ├─ Custom adapter template
        └─ Future: Rust server adapter
```

**Benefit**: Core has zero framework dependencies

---

## New Components Added (V3)

### 1. HTTP Abstraction Layer (10 hours)

**File**: `src/fraiseql/subscriptions/http_adapter.py` (~400 lines)

Two interfaces:

#### Interface 1: WebSocketAdapter
```python
class WebSocketAdapter(ABC):
    async def accept(subprotocol: str | None) -> None
    async def receive_json() -> dict
    async def send_json(data: dict) -> None
    async def send_bytes(data: bytes) -> None
    async def close(code: int, reason: str) -> None
    @property
    def is_connected() -> bool
```

**Implementations**:
- `FastAPIWebSocketAdapter` - Wraps FastAPI's WebSocket
- `StarletteWebSocketAdapter` - Wraps Starlette's WebSocket
- `CustomServerAdapter` - Template for custom frameworks

#### Interface 2: SubscriptionProtocolHandler
```python
class SubscriptionProtocolHandler(ABC):
    async def handle_connection(
        websocket: WebSocketAdapter,
        manager: SubscriptionManager,
        auth_handler: Callable | None
    ) -> None
```

**Implementation**:
- `GraphQLTransportWSHandler` - Implements graphql-transport-ws protocol

**Benefit**: Protocol logic in one place, testable independently

### 2. Framework-Specific Integrations (12 hours)

#### FastAPI Integration (4 hours)
**File**: `src/fraiseql/integrations/fastapi_subscriptions.py` (~150 lines)

```python
class SubscriptionRouterFactory:
    @staticmethod
    def create(manager, path="/graphql/subscriptions", auth_handler=None) -> APIRouter:
        router = APIRouter()
        handler = GraphQLTransportWSHandler()

        @router.websocket(path)
        async def websocket_endpoint(websocket):
            adapter = FastAPIWebSocketAdapter(websocket)
            await handler.handle_connection(adapter, manager, auth_handler)

        return router
```

Usage:
```python
manager = SubscriptionManager(config)
router = SubscriptionRouterFactory.create(manager)
app.include_router(router)
```

#### Starlette Integration (4 hours)
**File**: `src/fraiseql/integrations/starlette_subscriptions.py` (~150 lines)

```python
def create_subscription_app(app, manager, path="/graphql/subscriptions", auth_handler=None):
    handler = GraphQLTransportWSHandler()

    async def ws_endpoint(websocket):
        adapter = StarletteWebSocketAdapter(websocket)
        await handler.handle_connection(adapter, manager, auth_handler)

    route = WebSocketRoute(path, endpoint=ws_endpoint)
    app.routes.append(route)
```

Usage:
```python
app = Starlette()
create_subscription_app(app, manager)
```

#### Custom Server Example (4 hours)
**File**: `src/fraiseql/subscriptions/custom_server_example.py` (~80 lines)

Template showing how to implement WebSocketAdapter for ANY HTTP framework:

```python
class CustomServerWebSocketAdapter(WebSocketAdapter):
    def __init__(self, websocket_connection):
        self._conn = websocket_connection

    async def accept(self, subprotocol=None):
        await self._conn.accept(subprotocol)

    async def send_bytes(self, data: bytes):
        await self._conn.send(data)

    # ... etc

# Usage:
handler = GraphQLTransportWSHandler()
adapter = CustomServerAdapter(my_websocket)
await handler.handle_connection(adapter, manager, auth_handler)
```

---

## How V3 Enables Future Features

### When Rust HTTP Server is Ready

Currently (V2/V3):
```python
# Framework-specific (FastAPI example)
adapter = FastAPIWebSocketAdapter(fastapi_websocket)
await handler.handle_connection(adapter, manager, auth_handler)
```

When Rust server exists:
```rust
// In Rust HTTP server
#[handler]
async fn websocket(ws: WebSocket) {
    let adapter = RustWebSocketAdapter::new(ws);

    // Call same Python handler!
    Python::with_gil(|py| {
        handler.handle_connection(adapter, manager, auth_handler).await
    })
}
```

**No changes needed** to SubscriptionManager or protocol logic. Just implement one adapter.

### Adding New Frameworks

To add Quart, aiohttp, or any other:

1. Create adapter:
```python
class QuartWebSocketAdapter(WebSocketAdapter):
    def __init__(self, websocket):
        self._ws = websocket
    # Implement 5 methods...
```

2. Create integration:
```python
def create_subscription_app(app, manager):
    handler = GraphQLTransportWSHandler()

    @app.websocket("/graphql/subscriptions")
    async def ws(ws):
        adapter = QuartWebSocketAdapter(ws)
        await handler.handle_connection(adapter, manager)

    return app
```

3. Done! Zero changes to core SubscriptionManager.

### Supporting Multiple Protocols

Currently supports: graphql-transport-ws

To add graphql-ws:

```python
class GraphQLWSHandler(SubscriptionProtocolHandler):
    async def handle_connection(self, websocket, manager, auth_handler):
        # Different protocol, same WebSocketAdapter interface
```

No framework code needs changing.

---

## Phase 3 Timeline Comparison

### V2: 20 hours
```
3.1: SubscriptionManager (10 hours)
3.2: FastAPI Router (10 hours)
────────────────────────────────
Total: 20 hours
```

### V3: 30 hours
```
3.0: HTTP Abstraction Layer (10 hours)
3.1: SubscriptionManager - updated (8 hours)
3.2: Framework Integrations (12 hours)
     ├─ FastAPI (4 hours)
     ├─ Starlette (4 hours)
     └─ Custom template (4 hours)
────────────────────────────────────────
Total: 30 hours
```

**Net impact**: +10 hours = **130 hours total** (was 120 hours in V2)

---

## Why This Matters

### Aligns with User's Vision

User stated: "end goal is to have the fastest possible library, with Rust code everywhere it is possible, and allowing the library users to write only python code"

V3 enables this by:
- ✅ HTTP server choice (FastAPI, Starlette, **Rust** when ready)
- ✅ Framework agnostic core (users can use ANY framework)
- ✅ Zero framework knowledge in SubscriptionManager
- ✅ Future Rust server won't require porting Python code

### Performance Stays Consistent

**Same bottlenecks**:
- Event dispatch: Rust (fast)
- Security filtering: Rust (fast)
- Python resolver: One call per event (acceptable)
- Response pre-serialization: Rust (fast)

**Performance impact of abstraction**: Zero (abstraction layer used once at connection start, not in hot path)

### Testability Improves

**V2**: FastAPI-specific tests only
```python
# Must use real FastAPI WebSocket
async def test_with_fastapi():
    app = FastAPI()
    router = SubscriptionRouterFactory.create(manager)
    app.include_router(router)
    # Test with client...
```

**V3**: Framework-independent tests
```python
# Mock WebSocketAdapter, test protocol logic
class MockWebSocket(WebSocketAdapter):
    async def send_json(self, data):
        self.messages.append(data)

async def test_protocol():
    handler = GraphQLTransportWSHandler()
    ws = MockWebSocket()
    await handler.handle_connection(ws, manager)
    assert ws.messages[0]["type"] == "connection_ack"
```

---

## Summary

**V3 changes**:
1. Added HTTP abstraction layer (10 hours)
2. Split framework-specific code into integrations (12 hours)
3. Reduced SubscriptionManager complexity (8 vs 10 hours)
4. Added custom server template for future extensibility

**Result**:
- ✅ Same 4-week timeline
- ✅ Same performance
- ✅ Framework-agnostic core
- ✅ Ready for Rust HTTP server
- ✅ Easy to add new frameworks

**All three critical gaps from initial review are addressed**:
1. ✅ Async runtime lifecycle - Uses existing global tokio runtime
2. ✅ Event bus bridge design - Arc-based zero-copy, pre-serialized responses
3. ✅ WebSocket protocol handler - GraphQLTransportWSHandler (framework-agnostic)

Plus: **HTTP server abstraction** (new requirement from user)

---

## Files Documentation

### New Files Created

**Abstraction Layer**:
- `src/fraiseql/subscriptions/http_adapter.py` - WebSocketAdapter + protocol handlers

**Framework Integrations**:
- `src/fraiseql/integrations/fastapi_subscriptions.py` - FastAPI router
- `src/fraiseql/integrations/starlette_subscriptions.py` - Starlette integration
- `src/fraiseql/subscriptions/custom_server_example.py` - Custom server template

**Updated**:
- `src/fraiseql/subscriptions/manager.py` - Simplified, framework-agnostic

**Existing (unchanged behavior)**:
- `fraiseql_rs/src/subscriptions/py_bindings.rs` - PyO3 bindings (same as V2)
- `fraiseql_rs/src/subscriptions/executor.rs` - Event dispatcher (same as V2)

---

## Ready for Implementation

All architectural decisions finalized:
- ✅ HTTP abstraction enables future flexibility
- ✅ Framework-agnostic core maintains simplicity
- ✅ Performance targets unchanged
- ✅ Timeline remains 4 weeks / 130 hours
- ✅ All three critical gaps addressed
- ✅ Plus new requirement (HTTP server choice) integrated

**Status**: Ready to begin Phase 1 implementation
