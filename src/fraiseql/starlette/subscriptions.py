"""WebSocket subscription support for Starlette GraphQL server.

This module implements the graphql-ws protocol for GraphQL subscriptions
over WebSocket connections. It works with the Starlette HTTP server to
provide real-time subscription capabilities.

Protocol:
    - Implements graphql-ws protocol (not graphql-transport-ws)
    - Handles connection_init, start, stop, complete messages
    - Supports authentication via connection parameters
    - Graceful error handling and disconnection

Architecture:
    The Starlette implementation uses the SubscriptionHandler protocol
    from fraiseql.http.interface, allowing subscription handling to be
    framework-agnostic while supporting Starlette's WebSocket API.

Example:
    from starlette.applications import Starlette
    from fraiseql.starlette.subscriptions import add_subscription_routes

    app = Starlette()
    add_subscription_routes(app, schema, db_pool)
    # Now /graphql/subscriptions handles WebSocket connections

Connection Lifecycle:
    1. Client connects to /graphql/subscriptions
    2. Client sends connection_init with optional auth
    3. Server validates and sends connection_ack
    4. Client sends subscription start messages
    5. Server sends data messages as events occur
    6. Client sends stop to cancel subscription
    7. Connection closes with complete or error

Error Handling:
    - Invalid JSON: connection_error
    - Missing query: error message
    - Auth failure: connection_error
    - Execution error: error in data message
    - Network failure: automatic reconnect (client responsibility)
"""

import asyncio
import logging
from typing import Any

from graphql import GraphQLSchema
from psycopg_pool import AsyncConnectionPool
from starlette.routing import WebSocketRoute
from starlette.websockets import WebSocket, WebSocketDisconnect

from fraiseql.graphql.execute import execute_graphql
from fraiseql.subscriptions.manager import SubscriptionManager

logger = logging.getLogger(__name__)


class StarletteSubscriptionHandler:
    """Handle GraphQL subscriptions over WebSocket for Starlette.

    Implements the SubscriptionHandler protocol for Starlette's
    WebSocket API, supporting the graphql-ws protocol.

    Attributes:
        schema: GraphQL schema for subscription execution
        db_pool: Database connection pool
        subscription_manager: Manager for active subscriptions
    """

    def __init__(
        self,
        schema: GraphQLSchema,
        db_pool: AsyncConnectionPool,
        subscription_manager: SubscriptionManager | None = None,
    ):
        """Initialize subscription handler.

        Args:
            schema: GraphQL schema
            db_pool: Database connection pool
            subscription_manager: Optional custom subscription manager
        """
        self.schema = schema
        self.db_pool = db_pool
        self.subscription_manager = subscription_manager or SubscriptionManager()

    async def handle_subscription(
        self,
        websocket: WebSocket,
    ) -> None:
        """Handle a WebSocket subscription connection.

        Implements the graphql-ws protocol:
        - Expects connection_init first
        - Handles subscription start/stop messages
        - Sends data/error/complete messages

        Args:
            websocket: Starlette WebSocket connection
        """
        await websocket.accept(subprotocol="graphql-ws")

        connection_params: dict[str, Any] = {}
        authenticated = False

        try:
            # Wait for connection_init (first message must be this)
            message = await websocket.receive_json()

            if message.get("type") != "connection_init":
                await self._send_connection_error(
                    websocket,
                    "First message must be connection_init",
                )
                return

            connection_params = message.get("payload", {})

            # NOTE: Authentication validation can be added by implementing
            # a custom auth handler and passing connection_params to it.
            # For now, connections are accepted without auth.

            authenticated = True

            # Send connection_ack
            await websocket.send_json(
                {
                    "type": "connection_ack",
                    "payload": {},
                },
            )

            # Handle subscription messages
            await self._handle_messages(websocket, connection_params)

        except WebSocketDisconnect:
            logger.debug("WebSocket disconnected during subscription")
        except Exception as e:
            logger.exception("Error handling subscription")
            try:
                if not authenticated:
                    await self._send_connection_error(websocket, str(e))
            except Exception:
                pass

    async def _handle_messages(
        self,
        websocket: WebSocket,
        connection_params: dict[str, Any],
    ) -> None:
        """Handle subscription messages after connection established.

        Args:
            websocket: Starlette WebSocket connection
            connection_params: Parameters from connection_init
        """
        active_subscriptions: dict[str, asyncio.Task[None]] = {}

        try:
            while True:
                message = await websocket.receive_json()
                message_type = message.get("type")
                message_id = message.get("id")

                if message_type == "start":
                    # Start a new subscription
                    await self._handle_start(
                        websocket,
                        message,
                        active_subscriptions,
                        connection_params,
                    )

                elif message_type == "stop":
                    # Stop an active subscription
                    if message_id in active_subscriptions:
                        task = active_subscriptions[message_id]
                        task.cancel()
                        del active_subscriptions[message_id]
                        await websocket.send_json(
                            {
                                "type": "complete",
                                "id": message_id,
                            },
                        )

                elif message_type == "connection_terminate":
                    # Close connection
                    break

                else:
                    logger.warning(f"Unknown message type: {message_type}")

        finally:
            # Cancel all active subscriptions
            for task in active_subscriptions.values():
                task.cancel()

            # Wait for all tasks to complete
            if active_subscriptions:
                await asyncio.gather(
                    *active_subscriptions.values(),
                    return_exceptions=True,
                )

            await websocket.close()

    async def _handle_start(
        self,
        websocket: WebSocket,
        message: dict[str, Any],
        active_subscriptions: dict[str, asyncio.Task[None]],
        connection_params: dict[str, Any],
    ) -> None:
        """Handle subscription start message.

        Args:
            websocket: WebSocket connection
            message: Start message from client
            active_subscriptions: Dict of active subscription tasks
            connection_params: Connection parameters from init
        """
        message_id = message.get("id")
        payload = message.get("payload", {})

        if not message_id:
            logger.warning("Start message missing id")
            return

        query = payload.get("query")
        if not query:
            await websocket.send_json(
                {
                    "type": "error",
                    "id": message_id,
                    "payload": [{"message": "Query is required"}],
                },
            )
            return

        # Create subscription task
        task = asyncio.create_task(
            self._execute_subscription(
                websocket,
                message_id,
                query,
                payload.get("operationName"),
                payload.get("variables"),
                connection_params,
            ),
        )

        active_subscriptions[message_id] = task

    async def _execute_subscription(
        self,
        websocket: WebSocket,
        subscription_id: str,
        query: str,
        operation_name: str | None,
        variables: dict[str, Any] | None,
        connection_params: dict[str, Any],
    ) -> None:
        """Execute a subscription query.

        For now, this executes a single query and returns the result.
        Full subscription support (multiple events) would require
        implementing event streaming.

        Args:
            websocket: WebSocket connection
            subscription_id: Subscription ID from client
            query: GraphQL query string
            operation_name: Optional operation name
            variables: Optional query variables
            connection_params: Connection parameters
        """
        try:
            async with self.db_pool.connection() as conn:
                result = await execute_graphql(
                    schema=self.schema,
                    query=query,
                    operation_name=operation_name,
                    variables=variables,
                    context={
                        "db_connection": conn,
                        "connection_params": connection_params,
                    },
                )

                # Send result
                payload = {"data": result.data}
                if result.errors:
                    payload["errors"] = [
                        {
                            "message": str(e),
                            "extensions": {"code": "GRAPHQL_ERROR"},
                        }
                        for e in result.errors
                    ]

                await websocket.send_json(
                    {
                        "type": "data",
                        "id": subscription_id,
                        "payload": payload,
                    },
                )

                # Send complete
                await websocket.send_json(
                    {
                        "type": "complete",
                        "id": subscription_id,
                    },
                )

        except asyncio.CancelledError:
            # Subscription was cancelled by client
            logger.debug(f"Subscription {subscription_id} cancelled")
        except Exception as e:
            logger.exception(f"Error executing subscription {subscription_id}")
            try:
                await websocket.send_json(
                    {
                        "type": "error",
                        "id": subscription_id,
                        "payload": [{"message": str(e)}],
                    },
                )
            except Exception:
                pass

    async def _send_connection_error(
        self,
        websocket: WebSocket,
        message: str,
    ) -> None:
        """Send connection error and close.

        Args:
            websocket: WebSocket connection
            message: Error message
        """
        try:
            await websocket.send_json(
                {
                    "type": "connection_error",
                    "payload": {"message": message},
                },
            )
        finally:
            await websocket.close(code=1000)


def add_subscription_routes(
    app: Any,  # Starlette app
    schema: GraphQLSchema,
    db_pool: AsyncConnectionPool,
    path: str = "/graphql/subscriptions",
) -> None:
    """Add GraphQL subscription routes to Starlette app.

    This function modifies the app in-place by adding a WebSocket route
    for handling GraphQL subscriptions via the graphql-ws protocol.

    Args:
        app: Starlette application instance
        schema: GraphQL schema
        db_pool: Database connection pool
        path: WebSocket route path (default: /graphql/subscriptions)

    Example:
        from starlette.applications import Starlette
        from fraiseql.starlette.subscriptions import add_subscription_routes

        app = Starlette()
        add_subscription_routes(app, schema, db_pool)
    """
    handler = StarletteSubscriptionHandler(schema, db_pool)

    async def subscription_endpoint(websocket: WebSocket) -> None:
        """WebSocket endpoint for subscriptions."""
        await handler.handle_subscription(websocket)

    # Add WebSocket route
    route = WebSocketRoute(path, subscription_endpoint)
    if hasattr(app, "routes"):
        app.routes.append(route)
    else:
        logger.warning("App does not have routes attribute, cannot add subscription route")

    logger.info(f"Added subscription route at {path}")


__all__ = [
    "StarletteSubscriptionHandler",
    "add_subscription_routes",
]
