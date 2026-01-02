"""WebSocket subscription router factory for FraiseQL.

Provides convenient factory methods to create FastAPI routers with WebSocket
support for GraphQL subscriptions, with automatic protocol negotiation,
auth inheritance, and sensible defaults.
"""

import logging
from typing import Any, Callable

from fastapi import APIRouter, WebSocket, WebSocketDisconnect, WebSocketException
from graphql import GraphQLSchema

from fraiseql.config.websocket_config import WebSocketConfig
from fraiseql.subscriptions.websocket import SubscriptionManager

logger = logging.getLogger(__name__)

__all__ = ["SubscriptionRouterFactory"]


class SubscriptionRouterFactory:
    """Factory for creating WebSocket subscription routers.

    Handles:
    - Automatic protocol detection (graphql-ws, graphql-transport-ws)
    - Auth inheritance from FraiseQLConfig
    - Connection lifecycle management
    - Error handling with sensible defaults
    - Automatic metrics collection
    """

    @staticmethod
    def create(
        schema: GraphQLSchema,
        config: WebSocketConfig | None = None,
        path: str = "/graphql/subscriptions",
        auth_handler: Callable[[dict[str, Any]], dict[str, Any]] | None = None,
    ) -> APIRouter:
        """Create a FastAPI router with WebSocket subscription support.

        Args:
            schema: GraphQL schema with subscription types
            config: WebSocket configuration (uses development defaults if None)
            path: WebSocket endpoint path (default: /graphql/subscriptions)
            auth_handler: Optional async function to handle authentication

        Returns:
            FastAPI APIRouter with WebSocket endpoint

        Examples:
            # Minimal setup
            router = SubscriptionRouterFactory.create(schema)
            app.include_router(router)

            # Custom configuration
            from fraiseql.config import WebSocketPresets
            router = SubscriptionRouterFactory.create(
                schema,
                config=WebSocketPresets.PRODUCTION,
            )
            app.include_router(router)

            # With custom auth
            async def my_auth(params: dict) -> dict:
                token = params.get("authorization")
                user = await validate_token(token)
                return {"user_id": user.id}

            router = SubscriptionRouterFactory.create(
                schema,
                auth_handler=my_auth,
            )
            app.include_router(router)
        """
        router = APIRouter()

        # Use development defaults if no config provided
        if config is None:
            from fraiseql.config import WebSocketPresets

            config = WebSocketPresets.DEVELOPMENT

        # Ensure config is resolved
        if hasattr(config, "_resolved") and not config._resolved:
            config = config.resolve()

        # Create subscription manager
        manager = SubscriptionManager()
        manager.schema = schema
        manager.config = config

        @router.websocket(path)
        async def websocket_endpoint(websocket: WebSocket) -> None:
            """Handle WebSocket connection for subscriptions.

            Implements graphql-ws protocol with auto-detection of graphql-transport-ws.

            Lifecycle:
            1. Accept connection
            2. Wait for ConnectionInit
            3. Validate authentication (if required)
            4. Enter message loop (Subscribe, Complete, Ping, Pong)
            5. Stream subscription messages
            6. Cleanup on disconnect
            """
            connection_id = None

            try:
                # Step 1: Accept WebSocket connection
                try:
                    await websocket.accept(subprotocol="graphql-ws")
                except WebSocketException:
                    # Try without subprotocol (some clients don't support it)
                    try:
                        await websocket.accept()
                    except Exception as e:
                        logger.error(f"Failed to accept WebSocket: {e}")
                        raise

                logger.debug("WebSocket connection accepted")

                # Step 2: Delegate to manager for connection handling
                await manager.handle_connection(
                    websocket=websocket,
                    config=config,
                    auth_handler=auth_handler,
                )

            except WebSocketDisconnect:
                logger.debug(f"WebSocket disconnected: {connection_id}")

            except WebSocketException as e:
                logger.error(f"WebSocket error: {e}")
                try:
                    await websocket.close(code=1000, reason="Internal error")
                except Exception:
                    pass

            except Exception:
                logger.exception("Unexpected error in WebSocket handler")
                try:
                    await websocket.close(code=1011, reason="Internal server error")
                except Exception:
                    pass

        return router

    @staticmethod
    def create_with_auth(
        schema: GraphQLSchema,
        config: WebSocketConfig | None = None,
        auth_required: bool = True,
        path: str = "/graphql/subscriptions",
    ) -> APIRouter:
        """Create a router with authentication enforcement.

        Convenience method that creates a router with authentication
        automatically enforced based on config.

        Args:
            schema: GraphQL schema
            config: WebSocket configuration
            auth_required: Whether to require authentication
            path: WebSocket endpoint path

        Returns:
            FastAPI APIRouter with auth-enforcing WebSocket endpoint

        Examples:
            # Production setup with enforced auth
            router = SubscriptionRouterFactory.create_with_auth(
                schema,
                config=WebSocketPresets.PRODUCTION,
                auth_required=True,
            )
            app.include_router(router)
        """
        if config is None:
            from fraiseql.config import WebSocketPresets

            config = WebSocketPresets.PRODUCTION if auth_required else WebSocketPresets.DEVELOPMENT

        # Ensure config requires auth if requested
        if auth_required and config.require_authentication is False:
            # Create a modified config with auth required
            from dataclasses import replace

            config = replace(config, require_authentication=True)

        return SubscriptionRouterFactory.create(
            schema=schema,
            config=config,
            path=path,
        )

    @staticmethod
    def get_default_path(graphql_path: str = "/graphql") -> str:
        """Get the default WebSocket path based on HTTP GraphQL path.

        Args:
            graphql_path: HTTP GraphQL endpoint path

        Returns:
            Suggested WebSocket path

        Examples:
            >>> SubscriptionRouterFactory.get_default_path("/graphql")
            "/graphql/subscriptions"

            >>> SubscriptionRouterFactory.get_default_path("/api/graphql")
            "/api/graphql/subscriptions"
        """
        if graphql_path.endswith("/"):
            return f"{graphql_path}subscriptions"
        return f"{graphql_path}/subscriptions"
