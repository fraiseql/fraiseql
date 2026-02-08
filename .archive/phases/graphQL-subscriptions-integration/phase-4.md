# Phase 3: Python High-Level API - Implementation Plan

**Phase**: 3
**Objective**: Create framework-agnostic Python API with HTTP abstraction layer for FastAPI, Starlette, and custom servers
**Estimated Time**: 3 weeks / 30 hours
**Files Created**: 5 new Python files (~680 lines)
**Success Criteria**: SubscriptionManager works with FastAPI and Starlette, custom server adapter template complete
**Lead Engineer**: Junior Python Web Framework Developer

---

## Context

Phase 3 creates the user-facing Python API. Users write simple resolvers and setup code - everything else abstracted. HTTP abstraction allows any framework.

**Key Design Decisions**:
- Framework-agnostic SubscriptionManager
- WebSocketAdapter interface for HTTP abstraction
- GraphQLTransportWSHandler centralizes protocol logic
- Pre-serialized bytes sent directly to WebSocket (performance)

---

## Files to Create/Modify

### New Files
- `src/fraiseql/subscriptions/__init__.py` (NEW, ~20 lines)
- `src/fraiseql/subscriptions/manager.py` (NEW, ~300 lines) - SubscriptionManager
- `src/fraiseql/subscriptions/http_adapter.py` (NEW, ~400 lines) - Abstraction layer
- `src/fraiseql/integrations/fastapi_subscriptions.py` (NEW, ~150 lines) - FastAPI adapter
- `src/fraiseql/integrations/starlette_subscriptions.py` (NEW, ~150 lines) - Starlette adapter
- `src/fraiseql/subscriptions/custom_server_example.py` (NEW, ~80 lines) - Template

### Modified Files
- `src/fraiseql/integrations/__init__.py` (modify) - Add imports

---

## Detailed Implementation Tasks

### Task 3.0: HTTP Abstraction Layer (10 hours)

**Objective**: Create framework-agnostic interfaces for WebSocket operations

**File**: `src/fraiseql/subscriptions/http_adapter.py`

**Steps**:
1. Define WebSocketAdapter ABC
2. Implement FastAPIWebSocketAdapter
3. Implement StarletteWebSocketAdapter
4. Define SubscriptionProtocolHandler ABC
5. Implement GraphQLTransportWSHandler

**Code to Write**:

```python
# WebSocketAdapter interface
class WebSocketAdapter(ABC):
    """Abstract WebSocket interface implemented by each HTTP framework."""

    @abstractmethod
    async def accept(self, subprotocol: Optional[str] = None) -> None:
        """Accept WebSocket connection."""
        pass

    @abstractmethod
    async def receive_json(self) -> Dict[str, Any]:
        """Receive JSON message from client."""
        pass

    @abstractmethod
    async def send_json(self, data: Dict[str, Any]) -> None:
        """Send JSON message to client."""
        pass

    @abstractmethod
    async def send_bytes(self, data: bytes) -> None:
        """Send pre-serialized bytes to client (critical for performance)."""
        pass

    @abstractmethod
    async def close(self, code: int = 1000, reason: str = "") -> None:
        """Close connection gracefully."""
        pass

    @property
    @abstractmethod
    def is_connected(self) -> bool:
        """Check if WebSocket is still connected."""
        pass


# FastAPI implementation
class FastAPIWebSocketAdapter(WebSocketAdapter):
    """FastAPI WebSocket implementation."""

    def __init__(self, websocket: "WebSocket"):  # TYPE_CHECKING import
        self._ws = websocket

    async def accept(self, subprotocol: Optional[str] = None) -> None:
        await self._ws.accept(subprotocol=subprotocol)

    async def receive_json(self) -> Dict[str, Any]:
        return await self._ws.receive_json()

    async def send_json(self, data: Dict[str, Any]) -> None:
        await self._ws.send_json(data)

    async def send_bytes(self, data: bytes) -> None:
        await self._ws.send_bytes(data)

    async def close(self, code: int = 1000, reason: str = "") -> None:
        await self._ws.close(code=code, reason=reason)

    @property
    def is_connected(self) -> bool:
        return self._ws.client_state.value == 1  # FastAPI CONNECTED


# Starlette implementation
class StarletteWebSocketAdapter(WebSocketAdapter):
    """Starlette WebSocket implementation."""

    def __init__(self, websocket):
        self._ws = websocket

    async def accept(self, subprotocol: Optional[str] = None) -> None:
        await self._ws.accept(subprotocol=subprotocol)

    async def receive_json(self) -> Dict[str, Any]:
        # Starlette doesn't have receive_json, implement manually
        data = await self._ws.receive_text()
        return json.loads(data)

    async def send_json(self, data: Dict[str, Any]) -> None:
        await self._ws.send_json(data)

    async def send_bytes(self, data: bytes) -> None:
        await self._ws.send_bytes(data)

    async def close(self, code: int = 1000, reason: str = "") -> None:
        await self._ws.close(code=code, reason=reason)

    @property
    def is_connected(self) -> bool:
        return self._ws.client_state.value == 1  # Starlette CONNECTED


# Protocol handler interface
class SubscriptionProtocolHandler(ABC):
    """Protocol handler for different WebSocket protocols."""

    @abstractmethod
    async def handle_connection(
        self,
        websocket: WebSocketAdapter,
        manager: "SubscriptionManager",
        auth_handler: Optional[Callable] = None,
    ) -> None:
        """Handle complete WebSocket connection lifecycle."""
        pass


# GraphQL Transport WS implementation
class GraphQLTransportWSHandler(SubscriptionProtocolHandler):
    """Implements graphql-transport-ws protocol."""

    async def handle_connection(
        self,
        websocket: WebSocketAdapter,
        manager: "SubscriptionManager",
        auth_handler: Optional[Callable] = None,
    ) -> None:
        """Implement graphql-transport-ws connection lifecycle."""
        import asyncio
        from uuid import uuid4

        await websocket.accept(subprotocol="graphql-transport-ws")
        connection_id = str(uuid4())
        active_subscriptions: Dict[str, str] = {}
        listener_tasks: Dict[str, asyncio.Task] = {}

        try:
            while websocket.is_connected:
                try:
                    data = await websocket.receive_json()
                    msg_type = data.get("type")

                    if msg_type == "connection_init":
                        # Authentication
                        auth_data = data.get("payload", {})
                        if auth_handler:
                            user_context = await auth_handler(auth_data)
                        else:
                            user_context = {"user_id": "anonymous", "tenant_id": ""}

                        await websocket.send_json({"type": "connection_ack"})

                    elif msg_type == "subscribe":
                        sub_id = data.get("id")
                        payload = data.get("payload", {})

                        try:
                            # Register subscription
                            await manager.create_subscription(
                                subscription_id=sub_id,
                                connection_id=connection_id,
                                query=payload.get("query"),
                                operation_name=payload.get("operationName"),
                                variables=payload.get("variables", {}),
                                resolver_fn=self._get_resolver_for_query(payload.get("query")),
                                user_id=user_context.get("user_id"),
                                tenant_id=user_context.get("tenant_id", ""),
                            )

                            active_subscriptions[sub_id] = payload.get("query")

                            # Create listener task
                            task = asyncio.create_task(
                                self._listen_for_events(websocket, manager, sub_id)
                            )
                            listener_tasks[sub_id] = task

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

                        if sub_id in listener_tasks:
                            listener_tasks[sub_id].cancel()
                            del listener_tasks[sub_id]

                        await websocket.send_json({
                            "type": "complete",
                            "id": sub_id,
                        })

                    elif msg_type == "ping":
                        await websocket.send_json({"type": "pong"})

                except Exception as e:
                    await websocket.send_json({
                        "type": "error",
                        "payload": [{"message": f"Protocol error: {str(e)}"}],
                    })
                    break

        finally:
            # Cleanup
            for sub_id in active_subscriptions.keys():
                await manager.complete_subscription(sub_id)
            for task in listener_tasks.values():
                task.cancel()
            await websocket.close()

    def _get_resolver_for_query(self, query: str) -> Callable:
        """Extract resolver function from @subscription decorated functions."""
        # Parse query to find resolver
        # Return the decorated function
        pass

    async def _listen_for_events(
        self,
        websocket: WebSocketAdapter,
        manager: "SubscriptionManager",
        subscription_id: str,
    ) -> None:
        """Background task: listen for events and send to client."""
        while websocket.is_connected:
            try:
                response_bytes = await manager.get_next_event(subscription_id)

                if response_bytes:
                    # Send pre-serialized bytes directly (critical for performance)
                    await websocket.send_bytes(response_bytes)
                else:
                    # Wait before polling again
                    await asyncio.sleep(0.001)

            except asyncio.CancelledError:
                break
            except Exception as e:
                await websocket.send_json({
                    "type": "error",
                    "id": subscription_id,
                    "payload": [{"message": str(e)}],
                })
                break
```

**Acceptance Criteria**:
- [ ] WebSocketAdapter ABC defined
- [ ] FastAPIWebSocketAdapter implements all methods
- [ ] StarletteWebSocketAdapter implements all methods
- [ ] GraphQLTransportWSHandler implements protocol
- [ ] Protocol logic centralized (no framework-specific code)

### Task 3.1: Framework-Agnostic SubscriptionManager (8 hours)

**Objective**: Create the main user-facing class, framework-independent

**File**: `src/fraiseql/subscriptions/manager.py`

**Steps**:
1. Define SubscriptionManager class
2. Implement all methods using Phase 1 PyO3 bindings
3. Store subscription metadata in Python
4. Handle resolver function mapping

**Code to Write**:

```python
from typing import Optional, Dict, Any, Callable
from fraiseql import _fraiseql_rs
import asyncio


class SubscriptionManager:
    """Framework-agnostic subscription manager.

    Works with any HTTP framework via adapter pattern.
    All heavy lifting stays in Rust.
    """

    def __init__(
        self,
        event_bus_config: _fraiseql_rs.PyEventBusConfig,
    ):
        """Initialize with event bus configuration."""
        self.executor = _fraiseql_rs.PySubscriptionExecutor()
        self.event_bus_config = event_bus_config
        self.subscriptions: Dict[str, 'SubscriptionData'] = {}
        self._resolvers: Dict[str, Callable] = {}

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
        """Register a subscription (framework-agnostic)."""
        # Store metadata in Python
        self.subscriptions[subscription_id] = SubscriptionData(
            query=query,
            operation_name=operation_name,
            variables=variables,
            resolver_fn=resolver_fn,
            user_id=user_id,
            tenant_id=tenant_id,
        )

        # Register in Rust executor
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
        """Publish event (framework-agnostic)."""
        self.executor.publish_event(
            event_type=event_type,
            channel=channel,
            data=data,
        )

    async def get_next_event(
        self,
        subscription_id: str,
    ) -> Optional[bytes]:
        """Get next pre-serialized event bytes (framework-agnostic)."""
        return self.executor.next_event(subscription_id)

    async def complete_subscription(self, subscription_id: str) -> None:
        """Clean up subscription (framework-agnostic)."""
        self.executor.complete_subscription(subscription_id)
        if subscription_id in self.subscriptions:
            del self.subscriptions[subscription_id]

    def get_metrics(self) -> Dict[str, Any]:
        """Get metrics (framework-agnostic)."""
        return self.executor.get_metrics()

    # NEW: Resolver management
    def register_resolver(self, name: str, resolver_fn: Callable) -> None:
        """Register a resolver function."""
        self._resolvers[name] = resolver_fn

    def get_resolver(self, name: str) -> Optional[Callable]:
        """Get a registered resolver."""
        return self._resolvers.get(name)


class SubscriptionData:
    """Metadata for a subscription."""

    def __init__(
        self,
        query: str,
        operation_name: Optional[str],
        variables: Dict[str, Any],
        resolver_fn: Callable,
        user_id: str,
        tenant_id: str,
    ):
        self.query = query
        self.operation_name = operation_name
        self.variables = variables
        self.resolver_fn = resolver_fn
        self.user_id = user_id
        self.tenant_id = tenant_id
```

**Acceptance Criteria**:
- [ ] SubscriptionManager instantiates
- [ ] create_subscription stores metadata and calls Rust
- [ ] publish_event calls Rust executor
- [ ] get_next_event returns bytes from Rust
- [ ] complete_subscription cleans up both Python and Rust
- [ ] No framework-specific code

### Task 3.2: Framework-Specific Integrations (12 hours)

**Objective**: Create router/factory classes for FastAPI and Starlette

#### 3.2a: FastAPI Integration (4 hours)

**File**: `src/fraiseql/integrations/fastapi_subscriptions.py`

**Code to Write**:

```python
from fastapi import APIRouter, WebSocket
from fraiseql.subscriptions.http_adapter import (
    FastAPIWebSocketAdapter,
    GraphQLTransportWSHandler,
)
from fraiseql.subscriptions.manager import SubscriptionManager
from typing import Optional, Callable


class SubscriptionRouterFactory:
    """Create FastAPI router for subscriptions."""

    @staticmethod
    def create(
        manager: SubscriptionManager,
        path: str = "/graphql/subscriptions",
        auth_handler: Optional[Callable] = None,
    ) -> APIRouter:
        """Create FastAPI router.

        Usage:
            router = SubscriptionRouterFactory.create(manager)
            app.include_router(router)
        """
        router = APIRouter()
        handler = GraphQLTransportWSHandler()

        @router.websocket(path)
        async def websocket_endpoint(websocket: WebSocket):
            """WebSocket endpoint using protocol handler."""
            adapter = FastAPIWebSocketAdapter(websocket)
            await handler.handle_connection(adapter, manager, auth_handler)

        return router
```

#### 3.2b: Starlette Integration (4 hours)

**File**: `src/fraiseql/integrations/starlette_subscriptions.py`

**Code to Write**:

```python
from starlette.applications import Starlette
from starlette.routing import WebSocketRoute
from fraiseql.subscriptions.http_adapter import (
    StarletteWebSocketAdapter,
    GraphQLTransportWSHandler,
)
from fraiseql.subscriptions.manager import SubscriptionManager
from typing import Optional, Callable


async def subscription_websocket(websocket, manager, handler, auth_handler):
    """WebSocket handler for Starlette."""
    adapter = StarletteWebSocketAdapter(websocket)
    await handler.handle_connection(adapter, manager, auth_handler)


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
        await subscription_websocket(websocket, manager, handler, auth_handler)

    route = WebSocketRoute(path, endpoint=ws_endpoint)
    app.routes.append(route)
```

#### 3.2c: Custom Server Adapter (4 hours)

**File**: `src/fraiseql/subscriptions/custom_server_example.py`

**Code to Write**:

```python
"""Example: Custom HTTP server adapter.

Shows how to integrate subscriptions with ANY HTTP framework
by implementing WebSocketAdapter interface.
"""

from fraiseql.subscriptions.http_adapter import WebSocketAdapter
from typing import Optional, Dict, Any
import json


class CustomServerWebSocketAdapter(WebSocketAdapter):
    """Example adapter for custom HTTP server."""

    def __init__(self, websocket_connection):
        """Wrap your custom WebSocket connection."""
        self._conn = websocket_connection

    async def accept(self, subprotocol: Optional[str] = None) -> None:
        """Accept connection from your framework."""
        await self._conn.accept(subprotocol)

    async def receive_json(self) -> Dict[str, Any]:
        """Receive JSON from your framework."""
        data = await self._conn.receive()
        return json.loads(data)

    async def send_json(self, data: Dict[str, Any]) -> None:
        """Send JSON through your framework."""
        await self._conn.send(json.dumps(data))

    async def send_bytes(self, data: bytes) -> None:
        """Send pre-serialized bytes (critical for performance)."""
        await self._conn.send(data)

    async def close(self, code: int = 1000, reason: str = "") -> None:
        """Close connection."""
        await self._conn.close()

    @property
    def is_connected(self) -> bool:
        """Check connection status."""
        return self._conn.is_open


# Usage example:
# handler = GraphQLTransportWSHandler()
# adapter = CustomServerAdapter(my_websocket)
# await handler.handle_connection(adapter, manager, auth_handler)
```

---

## Task 4.4: Rollback and Recovery Procedures (2 hours)

**Objective**: Add rollback procedures and recovery guidance for production safety

#### Rollback Strategy
If deployment fails, follow these rollback procedures:

##### Immediate Rollback (0-5 minutes post-deployment)
```bash
# Stop the application
docker-compose down

# Revert to previous version
git checkout previous-tag
docker-compose up -d

# Verify rollback
curl http://localhost:8000/health
```

##### Database Rollback (if schema changes)
```sql
-- Revert any schema changes
-- Note: Phase 4 does not include schema changes
-- If added in future phases, include revert scripts
```

##### Configuration Rollback
```bash
# Revert environment variables
cp .env.backup .env

# Restart services
docker-compose restart
```

#### Recovery Procedures

##### After Successful Rollback
1. **Root Cause Analysis**
   - Check application logs
   - Verify system resources
   - Test in staging environment

2. **Fix Identification**
   - Reproduce issue locally
   - Apply fix with tests
   - Deploy to staging

3. **Gradual Rollout**
   - Deploy to 10% of traffic
   - Monitor metrics
   - Gradually increase traffic

##### Monitoring During Deployment
```bash
# Health checks
curl http://localhost:8000/health

# Performance metrics
curl http://localhost:8000/metrics

# Error rates
curl http://localhost:8000/errors
```

#### Contingency Planning

##### Risk: Performance Regression
- **Detection**: Automated benchmarks in CI/CD
- **Response**: Immediate rollback within 5 minutes
- **Prevention**: Performance tests in all environments

##### Risk: Breaking Changes
- **Detection**: Integration tests in CI/CD
- **Response**: Feature flags for gradual rollout
- **Prevention**: Comprehensive API versioning

##### Risk: Data Corruption
- **Detection**: Data validation checks
- **Response**: Database backup restoration
- **Prevention**: Read-only mode during deployment

---

## Testing Requirements

### Unit Tests (tests/test_subscriptions_phase3.py)

**Required Tests**:

```python
import pytest
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql.subscriptions.http_adapter import GraphQLTransportWSHandler
from fraiseql import _fraiseql_rs


@pytest.mark.asyncio
async def test_subscription_manager():
    """Test SubscriptionManager functionality."""
    config = _fraiseql_rs.PyEventBusConfig.memory()
    manager = SubscriptionManager(config)

    # Create subscription
    await manager.create_subscription(
        subscription_id="sub1",
        connection_id="conn1",
        query="subscription { test }",
        variables={},
        resolver_fn=lambda e, v: {"data": "test"},
        user_id="user1",
        tenant_id="tenant1",
    )

    # Publish event
    await manager.publish_event("test", "test", {"id": "123"})

    # Get event
    response = await manager.get_next_event("sub1")
    assert response is not None
    assert isinstance(response, bytes)


def test_websocket_adapter_interface():
    """Test WebSocketAdapter ABC."""
    # Test that adapters implement the interface
    pass


@pytest.mark.asyncio
async def test_protocol_handler():
    """Test GraphQLTransportWSHandler with mock adapter."""
    # Mock WebSocketAdapter
    # Test connection_init, subscribe, complete messages
    pass
```

### Integration Tests

**FastAPI Integration Test**:
```python
def test_fastapi_router_creation():
    manager = SubscriptionManager(config)
    router = SubscriptionRouterFactory.create(manager)
    assert router is not None
    # Verify route exists
```

**Starlette Integration Test**:
```python
def test_starlette_app_creation():
    app = Starlette()
    manager = SubscriptionManager(config)
    create_subscription_app(app, manager)
    assert len(app.routes) > 0
```

**Run Tests**:
```bash
pytest tests/test_subscriptions_phase3.py -v
```

---

## Verification Checklist

- [ ] All Python files import without errors
- [ ] SubscriptionManager works framework-independently
- [ ] FastAPI router creates correctly
- [ ] Starlette integration adds routes
- [ ] Custom adapter template compiles
- [ ] Protocol handler implements graphql-transport-ws
- [ ] Unit tests pass
- [ ] Type checking clean (mypy)

---

## Success Criteria for Phase 3

When Phase 3 is complete, users can do this:

**With FastAPI**:
```python
from fraiseql.subscriptions import SubscriptionManager
from fraiseql.integrations.fastapi_subscriptions import SubscriptionRouterFactory
from fraiseql import _fraiseql_rs

# Setup
event_bus_config = _fraiseql_rs.PyEventBusConfig.redis(url="redis://localhost:6379", consumer_group="test")
manager = SubscriptionManager(event_bus_config)

# Create router
router = SubscriptionRouterFactory.create(manager)
app.include_router(router)

# Done! WebSocket endpoint at /graphql/subscriptions
```

**With Starlette**:
```python
from fraiseql.integrations.starlette_subscriptions import create_subscription_app
from starlette.applications import Starlette

app = Starlette()
create_subscription_app(app, manager)
```

**Custom Server**:
```python
# Implement CustomServerAdapter following the template
# Use GraphQLTransportWSHandler with your adapter
```

---

## Blockers & Dependencies

**Prerequisites**:
- Phase 1 PyO3 bindings complete
- Phase 2 event dispatch complete
- FastAPI and Starlette available in environment

**Help Needed**:
- If framework WebSocket APIs unclear, ask senior engineer
- If protocol implementation details unclear, reference GraphQL spec
- If testing setup unclear, ask senior engineer

---

## Time Estimate Breakdown

- Task 3.0: 10 hours (HTTP abstraction layer)
- Task 3.1: 8 hours (SubscriptionManager)
- Task 3.2: 12 hours (Framework integrations: 4+4+4)
- Testing & fixes: 0 hours (covered in estimate)

**Total: 30 hours**

---

## Next Phase Dependencies

Phase 3 creates the Python API that Phase 4 will test end-to-end. Phase 3 must be complete and all framework integrations working before Phase 4 begins.</content>
<parameter name="filePath">/home/lionel/code/fraiseql/.phases/graphQL-subscriptions-integration/phase-3.md
