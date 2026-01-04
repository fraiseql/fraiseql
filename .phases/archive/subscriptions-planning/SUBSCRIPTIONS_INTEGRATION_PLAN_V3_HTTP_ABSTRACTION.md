# Subscriptions Integration - HTTP Server Abstraction Layer

**Date**: January 3, 2026
**Status**: Architectural Update
**Purpose**: Add pluggable HTTP server abstraction to support Rust default, Starlette base, FastAPI optional

---

## 🎯 New Requirement

The end goal includes dropping hardcoded FastAPI for flexible HTTP server choice:

- **Rust HTTP server** as DEFAULT
- **Starlette** as Python base default
- **FastAPI** as optional integration
- **Custom servers** should be possible via adapter pattern

This fundamentally changes Phase 3 architecture.

---

## Current Problem with V2 Plan

**What V2 assumes:**
- FastAPI hardcoded in `SubscriptionRouterFactory`
- WebSocket handling is FastAPI-specific
- Would require rewrite to support other frameworks

**What we need instead:**
- HTTP abstraction layer
- Framework-agnostic `SubscriptionManager`
- Pluggable WebSocket handlers
- Each framework implements its own adapter

---

## New Phase 3 Architecture

### 3.0: HTTP Abstraction Layer (NEW - 10 hours)

**File**: `src/fraiseql/subscriptions/http_adapter.py`

```python
"""
HTTP Server Abstraction Layer

Allows SubscriptionManager to work with any HTTP framework:
- Rust HTTP server (native)
- Starlette (Python base)
- FastAPI (convenience wrapper)
- Custom frameworks (via interface)
"""

from abc import ABC, abstractmethod
from typing import Optional, Callable, Dict, Any
import json

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
        """Send pre-serialized bytes to client.

        This is critical for performance - avoid JSON parse/serialize.
        """
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


class FastAPIWebSocketAdapter(WebSocketAdapter):
    """FastAPI WebSocket implementation."""

    def __init__(self, websocket):
        """Wrap FastAPI WebSocket."""
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
        return self._ws.client_state.value == 1  # CONNECTED


class StarletteWebSocketAdapter(WebSocketAdapter):
    """Starlette WebSocket implementation."""

    def __init__(self, websocket):
        """Wrap Starlette WebSocket."""
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
        return self._ws.client_state.value == 1  # CONNECTED


class SubscriptionProtocolHandler(ABC):
    """Protocol handler for different WebSocket protocols.

    Allows supporting multiple protocols:
    - graphql-ws (legacy)
    - graphql-transport-ws (current standard)
    - custom protocols
    """

    @abstractmethod
    async def handle_connection(
        self,
        websocket: WebSocketAdapter,
        manager: "SubscriptionManager",
        auth_handler: Optional[Callable] = None,
    ) -> None:
        """Handle complete WebSocket connection lifecycle."""
        pass


class GraphQLTransportWSHandler(SubscriptionProtocolHandler):
    """Implements graphql-transport-ws protocol."""

    async def handle_connection(
        self,
        websocket: WebSocketAdapter,
        manager: "SubscriptionManager",
        auth_handler: Optional[Callable] = None,
    ) -> None:
        """Implement graphql-transport-ws connection lifecycle.

        This is the protocol logic - framework-agnostic.
        Actual WebSocket operations use WebSocketAdapter.
        """
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
                            user_context = {"user_id": "anonymous"}

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
                                resolver_fn=get_resolver_for_query(payload.get("query")),
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
            # Cleanup on disconnect
            for sub_id in active_subscriptions.keys():
                await manager.complete_subscription(sub_id)
            for task in listener_tasks.values():
                task.cancel()
            await websocket.close()

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

---

### 3.1: Updated SubscriptionManager (8 hours, CHANGED)

**Key Changes:**
- Remove FastAPI-specific code
- Use `WebSocketAdapter` abstraction
- Framework-agnostic

```python
# src/fraiseql/subscriptions/manager.py (UPDATED)

from fraiseql import _fraiseql_rs
import asyncio
from typing import Optional, Dict, Any, Callable

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
        self.subscriptions[subscription_id] = SubscriptionData(
            query=query,
            operation_name=operation_name,
            variables=variables,
            resolver_fn=resolver_fn,
            user_id=user_id,
            tenant_id=tenant_id,
        )

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
```

---

### 3.2: Framework-Specific Integrations (12 hours, ADDED)

#### 3.2a: FastAPI Integration (4 hours)

**File**: `src/fraiseql/integrations/fastapi_subscriptions.py`

```python
"""FastAPI subscription integration.

Example usage:
    from fraiseql.subscriptions import SubscriptionManager
    from fraiseql.integrations.fastapi_subscriptions import SubscriptionRouterFactory
    from fraiseql import _fraiseql_rs

    # Setup
    event_bus_config = _fraiseql_rs.PyEventBusConfig.redis(...)
    manager = SubscriptionManager(event_bus_config)

    # Create router
    router = SubscriptionRouterFactory.create(manager)
    app.include_router(router)
"""

from fastapi import APIRouter, WebSocket, WebSocketDisconnect
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

```python
"""Starlette subscription integration.

Example usage:
    from fraiseql.subscriptions import SubscriptionManager
    from fraiseql.integrations.starlette_subscriptions import create_subscription_app
    from fraiseql import _fraiseql_rs
    from starlette.applications import Starlette

    # Setup
    event_bus_config = _fraiseql_rs.PyEventBusConfig.redis(...)
    manager = SubscriptionManager(event_bus_config)

    # Create app (can be included in larger app)
    app = Starlette()
    create_subscription_app(app, manager)
"""

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
# adapter = CustomServerWebSocketAdapter(my_websocket)
# await handler.handle_connection(adapter, manager, auth_handler)
```

---

## Updated Phase 3 Summary

**New Structure:**

```
PHASE 3: Python High-Level API (21 hours total)

3.0: HTTP Abstraction Layer (10 hours)
├── WebSocketAdapter interface
├── FastAPIWebSocketAdapter
├── StarletteWebSocketAdapter
├── SubscriptionProtocolHandler interface
└── GraphQLTransportWSHandler implementation

3.1: Framework-Agnostic SubscriptionManager (8 hours)
└── Zero framework-specific code

3.2: Framework-Specific Integrations (12 hours)
├── FastAPI integration (4 hours)
├── Starlette integration (4 hours)
└── Custom server examples (4 hours)
```

**Key Architecture Change:**

Before (V2):
```
SubscriptionManager → FastAPI-specific code
```

After (V3):
```
SubscriptionManager → WebSocketAdapter (abstraction)
                   → FastAPI adapter
                   → Starlette adapter
                   → Custom adapters...
```

---

## Benefits of This Architecture

✅ **Framework-Agnostic Core**: `SubscriptionManager` has zero framework dependencies

✅ **Rust HTTP Server Ready**: When Rust HTTP server is ready, just implement one more `WebSocketAdapter`

✅ **Protocol Abstraction**: Easy to support `graphql-ws`, `graphql-transport-ws`, custom protocols

✅ **Future-Proof**: Can add Sanic, Quart, aiohttp, etc. without changing core

✅ **Zero Duplicate Logic**: Protocol handling in one place (`GraphQLTransportWSHandler`)

✅ **Testing**: Mock `WebSocketAdapter` for testing without real framework

---

## Implementation Timeline (Updated)

**Phase 3 now 21 hours instead of 20:**
- 3.0 HTTP Abstraction: +10 hours
- 3.1 Manager (reduced): 8 hours
- 3.2 Framework integrations: +12 hours
- **Total Phase 3: 30 hours** (same as Phase 2, slightly increased from original)

**Overall timeline remains 4 weeks / 130 hours**

---

## Next Steps

1. **Review V3 architecture**
   - Does this match the vision of "choose your HTTP server"?
   - Any missing framework requirements?

2. **Prepare for Phase 1 implementation**
   - V2/V3 plan is now 75% architecture, ready to code
   - Phase 1 (PyO3 bindings) can start immediately
   - Phases 2-3 depend on Phase 1 completion

3. **Consider Rust HTTP Server Early**
   - This plan makes it trivial to add when ready
   - Just implement `WebSocketAdapter` in Rust
   - No changes needed to existing code

---

## Files Created by Phase 3 (Updated)

```
src/fraiseql/
├── subscriptions/
│   ├── __init__.py
│   ├── manager.py (UPDATED - no framework code)
│   ├── http_adapter.py (NEW - abstraction layer)
│   └── custom_server_example.py (NEW - reference)
└── integrations/
    ├── __init__.py
    ├── fastapi_subscriptions.py (NEW - FastAPI adapter)
    └── starlette_subscriptions.py (NEW - Starlette adapter)
```

---

## Conclusion

The new HTTP abstraction layer:
- ✅ Enables "choose your HTTP server" goal
- ✅ Prepares for future Rust HTTP server
- ✅ Centralizes protocol handling
- ✅ Maintains performance (pre-serialized bytes sent directly)
- ✅ Keeps timeline unchanged (4 weeks / 130 hours)

This is the final architectural piece needed before Phase 1 implementation begins.
