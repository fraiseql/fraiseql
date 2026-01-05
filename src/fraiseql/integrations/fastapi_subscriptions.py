"""FastAPI Integration for GraphQL Subscriptions (Phase 4).

Provides convenient factory methods to create FastAPI routers with WebSocket
support for GraphQL subscriptions, using the framework-agnostic abstractions
from Phase 4.

This module integrates:
- WebSocketAdapter abstraction (http_adapter.py)
- GraphQL Transport WebSocket protocol (protocol.py)
- SubscriptionManager high-level API (manager.py)

Example:
    from fraiseql.integrations.fastapi_subscriptions import create_subscription_router
    from fraiseql.subscriptions import SubscriptionManager
    from fastapi import FastAPI

    app = FastAPI()
    manager = SubscriptionManager()

    @manager.register_resolver("userCreated")
    async def on_user_created(event, variables):
        return {"user": event}

    router = create_subscription_router(manager)
    app.include_router(router)
"""

import logging
from collections.abc import Callable
from typing import Any

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from fraiseql.subscriptions.http_adapter import FastAPIWebSocketAdapter
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql.subscriptions.protocol import GraphQLTransportWSProtocol

logger = logging.getLogger(__name__)

__all__ = [
    "create_subscription_router",
    "create_subscription_router_with_auth",
]


def create_subscription_router(
    manager: SubscriptionManager,
    path: str = "/graphql/subscriptions",
    on_disconnect: Callable | None = None,
) -> APIRouter:
    """Create FastAPI router with WebSocket subscription endpoint.

    This function creates a FastAPI APIRouter with a WebSocket endpoint
    that integrates with the SubscriptionManager for handling GraphQL
    subscriptions.

    Args:
        manager: SubscriptionManager instance for handling subscriptions
        path: WebSocket endpoint path (default: /graphql/subscriptions)
        on_disconnect: Optional async callback when client disconnects

    Returns:
        FastAPI APIRouter ready to include in app

    Example:
        manager = SubscriptionManager()
        router = create_subscription_router(manager)
        app.include_router(router)

        # Clients can now connect to ws://localhost:8000/graphql/subscriptions
    """
    router = APIRouter()

    @router.websocket(path)
    async def websocket_endpoint(websocket: WebSocket) -> None:
        """Handle WebSocket subscription connection.

        Lifecycle:
        1. Accept WebSocket
        2. Create FastAPI adapter
        3. Initialize protocol handler
        4. Delegate to SubscriptionManager for message handling
        5. Cleanup on disconnect
        """
        try:
            # Create FastAPI WebSocket adapter for framework abstraction
            adapter = FastAPIWebSocketAdapter(websocket)

            # Create protocol handler for graphql-transport-ws
            protocol = GraphQLTransportWSProtocol()

            # Delegate to SubscriptionManager for connection handling
            # This manages the complete subscription lifecycle
            await manager.handle_connection(adapter, protocol)

        except WebSocketDisconnect:
            logger.debug("Client disconnected from subscriptions endpoint")
            if on_disconnect:
                await on_disconnect()

        except Exception:
            logger.exception("WebSocket subscription error")
            try:
                await websocket.close(code=1011, reason="Internal error")
            except Exception:
                pass

    return router


def create_subscription_router_with_auth(
    manager: SubscriptionManager,
    auth_handler: Callable[[dict[str, Any]], Any],
    path: str = "/graphql/subscriptions",
) -> APIRouter:
    """Create subscription router with authentication enforcement.

    This function creates a router that validates authentication tokens
    before allowing subscription connections.

    Args:
        manager: SubscriptionManager instance
        auth_handler: Async function to validate auth tokens.
                     Called with connection_init payload, should return
                     auth context dict or raise exception.
        path: WebSocket endpoint path

    Returns:
        FastAPI APIRouter with auth-enforcing endpoint

    Example:
        async def validate_token(params: dict) -> dict:
            token = params.get("authorization")
            if not token:
                raise ValueError("Missing authorization token")
            user = await get_user_from_token(token)
            if not user:
                raise ValueError("Invalid token")
            return {"user_id": user.id, "username": user.name}

        router = create_subscription_router_with_auth(
            manager,
            auth_handler=validate_token,
        )
        app.include_router(router)
    """
    router = APIRouter()

    @router.websocket(path)
    async def websocket_endpoint(websocket: WebSocket) -> None:
        """Handle authenticated WebSocket subscription connection."""
        try:
            # Create adapter
            adapter = FastAPIWebSocketAdapter(websocket)

            # Accept connection
            await adapter.accept(subprotocol="graphql-transport-ws")

            # Receive first message (should be connection_init)
            message = await adapter.receive_json()

            if message.get("type") != "connection_init":
                # Protocol violation - close connection
                await adapter.send_json(
                    {
                        "type": "connection_error",
                        "payload": {
                            "message": "First message must be connection_init",
                        },
                    }
                )
                await adapter.close()
                return

            # Validate authentication using provided handler
            auth_params = message.get("payload", {})
            try:
                auth_context = await auth_handler(auth_params)
            except Exception as e:
                # Auth failed - close connection
                error_msg = str(e) or "Authentication failed"
                await adapter.send_json(
                    {
                        "type": "connection_error",
                        "payload": {"message": error_msg},
                    }
                )
                await adapter.close()
                logger.warning(f"Authentication failed: {error_msg}")
                return

            # Create protocol handler
            protocol = GraphQLTransportWSProtocol()

            # Send connection_ack with auth context
            await adapter.send_json(
                {
                    "type": "connection_ack",
                    "payload": auth_context,
                }
            )

            # Mark protocol as ready (skip re-accepting connection)
            protocol.state = "ready"

            # Continue with message loop (must be implemented in manager)
            # For now, use the standard handle_connection but skip the accept
            await _handle_authenticated_connection(
                adapter,
                manager,
                protocol,
            )

        except WebSocketDisconnect:
            logger.debug("Authenticated client disconnected")

        except Exception:
            logger.exception("Authenticated WebSocket error")
            try:
                await websocket.close(code=1011)
            except Exception as close_e:
                logger.debug(f"Failed to close websocket on error: {close_e}")

    return router


async def _handle_authenticated_connection(
    adapter: FastAPIWebSocketAdapter,
    manager: SubscriptionManager,
    protocol: GraphQLTransportWSProtocol,
) -> None:
    """Handle message loop for authenticated connection.

    Args:
        adapter: FastAPI WebSocket adapter
        manager: SubscriptionManager
        protocol: Protocol handler (already in ready state)
    """
    try:
        while adapter.is_connected:
            try:
                message = await adapter.receive_json()
                await manager._handle_protocol_message(adapter, message)
            except Exception as e:
                logger.exception("Error handling message")
                await protocol.send_error(adapter, None, str(e))
    finally:
        try:
            await adapter.close()
        except Exception as e:
            logger.debug(f"Failed to close adapter in finally: {e}")
