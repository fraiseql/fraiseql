"""HTTP Framework Abstraction Layer for WebSocket Subscriptions.

This module provides framework-agnostic WebSocket and protocol handler abstractions,
enabling FraiseQL subscriptions to work with FastAPI, Starlette, and custom servers.

Key Components:
- WebSocketAdapter: Abstract interface for WebSocket operations
- FastAPIWebSocketAdapter: FastAPI implementation
- StarletteWebSocketAdapter: Starlette implementation
- SubscriptionProtocolHandler: Abstract protocol handler interface
"""

import json
import logging
from abc import ABC, abstractmethod
from typing import Any

logger = logging.getLogger(__name__)


# ============================================================================
# WebSocket Adapter - Framework Abstraction Layer
# ============================================================================


class WebSocketAdapter(ABC):
    """Abstract WebSocket interface for framework-agnostic operations.

    All HTTP frameworks that want to support FraiseQL subscriptions must
    provide an implementation of this interface.

    This allows subscription logic to be framework-independent.
    """

    @abstractmethod
    async def accept(self, subprotocol: str | None = None) -> None:
        """Accept WebSocket connection from client.

        Args:
            subprotocol: Optional subprotocol name (e.g., "graphql-ws")

        Raises:
            Exception: If connection cannot be accepted
        """
        ...

    @abstractmethod
    async def receive_json(self) -> dict[str, Any]:
        """Receive and parse JSON message from client.

        Returns:
            Parsed JSON object from client

        Raises:
            Exception: On connection error or invalid JSON
        """
        ...

    @abstractmethod
    async def send_json(self, data: dict[str, Any]) -> None:
        """Send JSON message to client.

        Args:
            data: Dictionary to serialize and send as JSON

        Raises:
            Exception: On connection error
        """
        ...

    @abstractmethod
    async def send_bytes(self, data: bytes) -> None:
        """Send pre-serialized bytes to client.

        This method is critical for performance as responses are pre-serialized
        in the Rust core and delivered directly to WebSocket without additional
        processing.

        Args:
            data: Pre-serialized bytes to send

        Raises:
            Exception: On connection error
        """
        ...

    @abstractmethod
    async def close(self, code: int = 1000, reason: str = "") -> None:
        """Close WebSocket connection gracefully.

        Args:
            code: WebSocket close code (1000 for normal closure)
            reason: Optional close reason message

        Raises:
            Exception: If close fails
        """
        ...

    @property
    @abstractmethod
    def is_connected(self) -> bool:
        """Check if WebSocket is still connected.

        Returns:
            True if connection is active, False otherwise
        """
        ...


# ============================================================================
# FastAPI Implementation
# ============================================================================


class FastAPIWebSocketAdapter(WebSocketAdapter):
    """FastAPI WebSocket adapter implementation.

    Wraps FastAPI's WebSocket class to provide the unified WebSocketAdapter
    interface, allowing framework-agnostic subscription handling.
    """

    def __init__(self, websocket: Any) -> None:
        """Initialize adapter with FastAPI WebSocket.

        Args:
            websocket: FastAPI WebSocket instance from route handler
        """
        self._ws = websocket

    async def accept(self, subprotocol: str | None = None) -> None:
        """Accept FastAPI WebSocket connection."""
        await self._ws.accept(subprotocol=subprotocol)
        logger.debug(f"FastAPI WebSocket accepted with subprotocol: {subprotocol}")

    async def receive_json(self) -> dict[str, Any]:
        """Receive JSON from FastAPI WebSocket."""
        data = await self._ws.receive_json()
        logger.debug(f"FastAPI received JSON: {data}")
        return data

    async def send_json(self, data: dict[str, Any]) -> None:
        """Send JSON via FastAPI WebSocket."""
        await self._ws.send_json(data)
        logger.debug(f"FastAPI sent JSON: {data}")

    async def send_bytes(self, data: bytes) -> None:
        """Send pre-serialized bytes via FastAPI WebSocket."""
        await self._ws.send_bytes(data)
        logger.debug(f"FastAPI sent {len(data)} bytes")

    async def close(self, code: int = 1000, reason: str = "") -> None:
        """Close FastAPI WebSocket."""
        await self._ws.close(code=code, reason=reason)
        logger.debug(f"FastAPI WebSocket closed with code {code}: {reason}")

    @property
    def is_connected(self) -> bool:
        """Check FastAPI WebSocket connection status."""
        # FastAPI WebSocket has client_state that can be checked
        # CONNECTED = 1, DISCONNECTED = 0
        return hasattr(self._ws, "client_state") and self._ws.client_state.value == 1


# ============================================================================
# Starlette Implementation
# ============================================================================


class StarletteWebSocketAdapter(WebSocketAdapter):
    """Starlette WebSocket adapter implementation.

    Wraps Starlette's WebSocket class to provide the unified WebSocketAdapter
    interface. Note that Starlette's WebSocket API is slightly different from
    FastAPI (which is based on Starlette), so we handle those differences here.
    """

    def __init__(self, websocket: Any) -> None:
        """Initialize adapter with Starlette WebSocket.

        Args:
            websocket: Starlette WebSocket instance from ASGI app
        """
        self._ws = websocket

    async def accept(self, subprotocol: str | None = None) -> None:
        """Accept Starlette WebSocket connection."""
        await self._ws.accept(subprotocol=subprotocol)
        logger.debug(f"Starlette WebSocket accepted with subprotocol: {subprotocol}")

    async def receive_json(self) -> dict[str, Any]:
        """Receive JSON from Starlette WebSocket.

        Starlette doesn't have receive_json(), so we receive text and parse.
        """
        text = await self._ws.receive_text()
        data = json.loads(text)
        logger.debug(f"Starlette received JSON: {data}")
        return data

    async def send_json(self, data: dict[str, Any]) -> None:
        """Send JSON via Starlette WebSocket."""
        text = json.dumps(data)
        await self._ws.send_text(text)
        logger.debug(f"Starlette sent JSON: {data}")

    async def send_bytes(self, data: bytes) -> None:
        """Send pre-serialized bytes via Starlette WebSocket."""
        await self._ws.send_bytes(data)
        logger.debug(f"Starlette sent {len(data)} bytes")

    async def close(self, code: int = 1000, reason: str = "") -> None:
        """Close Starlette WebSocket."""
        await self._ws.close(code=code, reason=reason)
        logger.debug(f"Starlette WebSocket closed with code {code}: {reason}")

    @property
    def is_connected(self) -> bool:
        """Check Starlette WebSocket connection status."""
        # Starlette WebSocket has client_state
        # CONNECTED = 1, DISCONNECTED = 0
        return hasattr(self._ws, "client_state") and self._ws.client_state.value == 1


# ============================================================================
# Custom Server Adapter Template
# ============================================================================


class CustomServerWebSocketAdapter(WebSocketAdapter):
    """Template for implementing custom server adapter.

    To implement subscriptions in a custom server, create a class that
    inherits from WebSocketAdapter and implements all abstract methods.

    Example:
        class MyServerAdapter(WebSocketAdapter):
            def __init__(self, connection):
                self.connection = connection

            async def accept(self, subprotocol=None):
                await self.connection.accept_ws(subprotocol)

            async def receive_json(self):
                message = await self.connection.receive()
                return json.loads(message)

            # ... implement other methods
    """

    def __init__(self, connection: Any) -> None:
        """Initialize with custom server connection object.

        Args:
            connection: Your server's connection object
        """
        self._connection = connection

    async def accept(self, subprotocol: str | None = None) -> None:
        """Accept connection in your server."""
        raise NotImplementedError(
            "Implement accept() for your server: await your_server.accept_websocket(subprotocol)",
        )

    async def receive_json(self) -> dict[str, Any]:
        """Receive JSON from your server."""
        raise NotImplementedError(
            "Implement receive_json() for your server",
        )

    async def send_json(self, data: dict[str, Any]) -> None:
        """Send JSON via your server."""
        raise NotImplementedError(
            "Implement send_json() for your server",
        )

    async def send_bytes(self, data: bytes) -> None:
        """Send bytes via your server."""
        raise NotImplementedError(
            "Implement send_bytes() for your server",
        )

    async def close(self, code: int = 1000, reason: str = "") -> None:
        """Close connection in your server."""
        raise NotImplementedError(
            "Implement close() for your server",
        )

    @property
    def is_connected(self) -> bool:
        """Check if connection is still active in your server."""
        raise NotImplementedError(
            "Implement is_connected property for your server",
        )


# ============================================================================
# Protocol Handler Interface
# ============================================================================


class SubscriptionProtocolHandler(ABC):
    """Abstract protocol handler for subscription lifecycle.

    Handles the graphql-transport-ws protocol which defines how clients
    communicate with servers for GraphQL subscriptions.
    """

    @abstractmethod
    async def handle_connection_init(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Handle connection_init message from client.

        Args:
            adapter: WebSocket adapter for sending response
            message: The connection_init message

        Raises:
            Exception: If initialization fails
        """
        ...

    @abstractmethod
    async def handle_subscribe(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Handle subscribe message from client.

        Args:
            adapter: WebSocket adapter
            message: The subscribe message containing query and variables

        Raises:
            Exception: If subscription creation fails
        """
        ...

    @abstractmethod
    async def handle_complete(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Handle complete message from client.

        Args:
            adapter: WebSocket adapter
            message: The complete message

        Raises:
            Exception: If cleanup fails
        """
        ...

    @abstractmethod
    async def send_next(
        self,
        adapter: WebSocketAdapter,
        subscription_id: str,
        data: dict[str, Any],
    ) -> None:
        """Send next message with subscription data to client.

        Args:
            adapter: WebSocket adapter
            subscription_id: Subscription identifier
            data: Subscription response data

        Raises:
            Exception: If send fails
        """
        ...

    @abstractmethod
    async def send_error(
        self,
        adapter: WebSocketAdapter,
        subscription_id: str | None,
        error_message: str,
    ) -> None:
        """Send error message to client.

        Args:
            adapter: WebSocket adapter
            subscription_id: Subscription identifier (None for connection errors)
            error_message: Error description

        Raises:
            Exception: If send fails
        """
        ...

    @abstractmethod
    async def send_complete(
        self,
        adapter: WebSocketAdapter,
        subscription_id: str,
    ) -> None:
        """Send complete message to client.

        Args:
            adapter: WebSocket adapter
            subscription_id: Subscription identifier

        Raises:
            Exception: If send fails
        """
        ...


# ============================================================================
# Utility Functions
# ============================================================================


def create_adapter(websocket: Any, framework: str = "auto") -> WebSocketAdapter:
    """Factory function to create appropriate adapter for framework.

    Args:
        websocket: Framework WebSocket object
        framework: "fastapi", "starlette", or "auto" for auto-detection

    Returns:
        Appropriate WebSocketAdapter implementation

    Raises:
        ValueError: If framework is unknown or detection fails
    """
    if framework == "auto":
        # Auto-detect based on class name
        class_name = websocket.__class__.__name__
        if "fastapi" in class_name.lower() or "starlette" in class_name.lower():
            framework = "fastapi"  # FastAPI is based on Starlette
        else:
            raise ValueError(f"Cannot auto-detect framework for {class_name}")

    if framework.lower() in ("fastapi", "starlette"):
        return FastAPIWebSocketAdapter(websocket)
    raise ValueError(f"Unknown framework: {framework}")


__all__ = [
    "CustomServerWebSocketAdapter",
    "FastAPIWebSocketAdapter",
    "StarletteWebSocketAdapter",
    "SubscriptionProtocolHandler",
    "WebSocketAdapter",
    "create_adapter",
]
