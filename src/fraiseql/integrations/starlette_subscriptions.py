"""Starlette Integration for GraphQL Subscriptions.

Provides ASGI middleware and route handlers for adding GraphQL
subscriptions to Starlette-based applications (including FastAPI).

This uses the framework-agnostic SubscriptionManager via the
WebSocketAdapter abstraction layer.

Example:
    from fraiseql.integrations.starlette_subscriptions import add_subscription_routes
    from fraiseql.subscriptions import SubscriptionManager
    from starlette.applications import Starlette

    app = Starlette()
    manager = SubscriptionManager()

    add_subscription_routes(app, manager)
"""

import logging
from typing import Any, Callable

from fraiseql.subscriptions.http_adapter import StarletteWebSocketAdapter
from fraiseql.subscriptions.manager import SubscriptionManager
from fraiseql.subscriptions.protocol import GraphQLTransportWSProtocol

logger = logging.getLogger(__name__)


def add_subscription_routes(
    app: any,  # Starlette.applications.Starlette
    manager: SubscriptionManager,
    path: str = "/graphql/subscriptions",
) -> None:
    """Add WebSocket subscription routes to Starlette app.

    This function modifies the app in-place by adding a WebSocket route
    for handling GraphQL subscriptions.

    Args:
        app: Starlette application instance
        manager: SubscriptionManager instance
        path: WebSocket route path

    Example:
        from starlette.applications import Starlette
        app = Starlette()
        manager = SubscriptionManager()
        add_subscription_routes(app, manager)

        # Now app can handle subscription connections at /graphql/subscriptions
    """

    async def websocket_endpoint(
        scope: dict[str, Any],
        receive: Callable[[], Any],
        send: Callable[[dict[str, Any]], Any],
    ) -> None:
        """ASGI WebSocket endpoint handler."""
        try:
            # Create Starlette WebSocket adapter
            from starlette.websockets import WebSocket as StarletteWS

            websocket = StarletteWS(scope, receive, send)
            adapter = StarletteWebSocketAdapter(websocket)

            # Create protocol handler
            protocol = GraphQLTransportWSProtocol()

            # Handle connection
            await manager.handle_connection(adapter, protocol)

        except Exception:
            logger.exception("Starlette WebSocket subscription error")

    # Add route to app
    app.router.routes.append(
        _create_route(path, websocket_endpoint)
    )
    logger.info(f"Added subscription route at {path}")


def add_subscription_routes_with_auth(
    app: any,  # Starlette.applications.Starlette
    manager: SubscriptionManager,
    auth_handler: Callable,
    path: str = "/graphql/subscriptions",
) -> None:
    """Add authenticated WebSocket subscription routes.

    Args:
        app: Starlette application instance
        manager: SubscriptionManager instance
        auth_handler: Async function to validate auth tokens
        path: WebSocket route path

    Example:
        async def validate_token(params: dict) -> dict:
            token = params.get("authorization")
            user = await get_user_from_token(token)
            return {"user_id": user.id}

        add_subscription_routes_with_auth(
            app,
            manager,
            auth_handler=validate_token,
        )
    """

    async def websocket_endpoint(
        scope: dict[str, Any],
        receive: Callable[[], Any],
        send: Callable[[dict[str, Any]], Any],
    ) -> None:
        """ASGI WebSocket endpoint with authentication."""
        try:
            from starlette.websockets import WebSocket as StarletteWS

            websocket = StarletteWS(scope, receive, send)
            adapter = StarletteWebSocketAdapter(websocket)

            # Accept connection
            await adapter.accept(subprotocol="graphql-transport-ws")

            # Get first message (should be connection_init)
            message = await adapter.receive_json()

            if message.get("type") != "connection_init":
                await adapter.send_json({
                    "type": "connection_error",
                    "payload": {
                        "message": "First message must be connection_init"
                    },
                })
                await adapter.close()
                return

            # Validate authentication
            auth_params = message.get("payload", {})
            try:
                auth_context = await auth_handler(auth_params)
            except Exception as e:
                await adapter.send_json({
                    "type": "connection_error",
                    "payload": {
                        "message": f"Authentication failed: {e!s}"
                    },
                })
                await adapter.close()
                return

            # Send connection_ack
            await adapter.send_json({
                "type": "connection_ack",
                "payload": {"user": auth_context},
            })

            # Create protocol and continue
            protocol = GraphQLTransportWSProtocol()
            protocol.state = "ready"

            # Continue handling messages
            await _handle_authenticated_messages(
                adapter,
                manager,
                protocol,
            )

        except Exception:
            logger.exception("Authenticated Starlette WebSocket error")

    # Add route
    app.router.routes.append(
        _create_route(path, websocket_endpoint)
    )
    logger.info(f"Added authenticated subscription route at {path}")


async def _handle_authenticated_messages(
    adapter: StarletteWebSocketAdapter,
    manager: SubscriptionManager,
    protocol: GraphQLTransportWSProtocol,
) -> None:
    """Continue handling messages for authenticated connection.

    Args:
        adapter: WebSocket adapter
        manager: SubscriptionManager
        protocol: Protocol handler (already initialized)
    """
    try:
        while adapter.is_connected:
            try:
                message = await adapter.receive_json()
                await manager._handle_protocol_message(adapter, message)
            except Exception as e:
                logger.exception("Message handling error")
                if protocol:
                    await protocol.send_error(adapter, None, str(e))
    except Exception:
        pass
    finally:
        await adapter.close()


def _create_route(path: str, endpoint: Callable) -> Any:
    """Create a Starlette WebSocket route.

    Args:
        path: Route path
        endpoint: ASGI endpoint handler

    Returns:
        Starlette Route object
    """
    from starlette.routing import Route

    # Create a simple ASGI app wrapper
    async def app(scope: dict, receive: Callable, send: Callable) -> None:
        await endpoint(scope, receive, send)

    return Route(path, endpoint=app, name="subscription")


__all__ = [
    "add_subscription_routes",
    "add_subscription_routes_with_auth",
]
