"""High-Level Python SubscriptionManager API.

Provides a simple, framework-agnostic interface for managing GraphQL subscriptions.
Users interact with this API; the low-level Rust executor and protocol handlers
are abstracted away.

Key Features:
- Simple resolver registration (@manager.register_resolver decorator)
- Event publishing (manager.publish_event)
- Framework-agnostic (works with FastAPI, Starlette, custom servers)
- Automatic GIL management for Python/Rust boundary
- Type-safe with complete type hints
"""

import asyncio
import logging
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Any

from fraiseql.subscriptions.http_adapter import (
    SubscriptionProtocolHandler,
    WebSocketAdapter,
)
from fraiseql.subscriptions.protocol import (
    GraphQLTransportWSProtocol,
    ProtocolStateMachine,
)

logger = logging.getLogger(__name__)


# ============================================================================
# Type Definitions
# ============================================================================


ResolverFunction = Callable[
    [dict[str, Any], dict[str, Any]],
    Awaitable[dict[str, Any]],
]
"""Type hint for resolver functions.

Args:
    event_data: Raw event data from publish_event
    variables: GraphQL variables from subscription query

Returns:
    Transformed data to send in next message
"""


# ============================================================================
# Configuration
# ============================================================================


@dataclass
class SubscriptionConfig:
    """Configuration for SubscriptionManager."""

    max_subscriptions_per_connection: int = 100
    """Maximum subscriptions allowed per WebSocket connection."""

    max_message_size: int = 1024 * 1024  # 1MB
    """Maximum message size in bytes."""

    connection_timeout: float = 10.0
    """Timeout for connection initialization in seconds."""

    keep_alive_interval: float = 30.0
    """Keep-alive ping interval in seconds (0 to disable)."""

    enable_compression: bool = False
    """Enable WebSocket compression (requires server support)."""

    log_level: str = "INFO"
    """Logging level for subscription operations."""


# ============================================================================
# SubscriptionManager
# ============================================================================


class SubscriptionManager:
    """High-level Python API for GraphQL subscriptions.

    This is the main entry point for users. They register resolvers and
    publish events through this interface, which delegates to the Rust
    executor and protocol handlers.

    Example:
        manager = SubscriptionManager()

        @manager.register_resolver("userCreated")
        async def resolve_user(event, variables):
            return {"user": {"id": event["user_id"]}}

        # In route handler:
        await manager.handle_connection(websocket, protocol_handler)

        # When publishing events:
        await manager.publish_event("userCreated", "users", {
            "user_id": "123",
            "name": "Alice"
        })
    """

    def __init__(self, config: SubscriptionConfig | None = None) -> None:
        """Initialize SubscriptionManager.

        Args:
            config: Optional configuration (uses defaults if None)
        """
        self.config = config or SubscriptionConfig()
        self.resolvers: dict[str, ResolverFunction] = {}
        self.subscriptions: dict[str, dict[str, Any]] = {}
        self.protocol: SubscriptionProtocolHandler | None = None
        self.state_machine: ProtocolStateMachine | None = None

        logger.setLevel(self.config.log_level)
        logger.info("SubscriptionManager initialized")

    def register_resolver(self, event_type: str) -> Callable:
        """Decorator to register a resolver function.

        Args:
            event_type: Name of event type (e.g., "userCreated")

        Returns:
            Decorator function

        Example:
            @manager.register_resolver("userCreated")
            async def resolve_user(event, variables):
                return {"user": event}
        """

        def decorator(func: ResolverFunction) -> ResolverFunction:
            """Apply decorator."""
            self.resolvers[event_type] = func
            logger.info(f"Registered resolver for event: {event_type}")
            return func

        return decorator

    async def publish_event(
        self,
        event_type: str,
        channel: str,
        data: dict[str, Any],
    ) -> None:
        """Publish an event to subscriptions.

        Args:
            event_type: Type of event (e.g., "userCreated")
            channel: Event channel/topic (e.g., "users")
            data: Event data to transform via resolver

        Example:
            await manager.publish_event("userCreated", "users", {
                "user_id": "123",
                "name": "Alice",
                "email": "alice@example.com"
            })
        """
        logger.debug(f"Publishing {event_type} to {channel}")

        # Get resolver for this event type
        resolver = self.resolvers.get(event_type)
        if not resolver:
            logger.warning(f"No resolver registered for {event_type}")
            return

        # Invoke resolver for each active subscription
        for sub_id, sub_info in list(self.subscriptions.items()):
            if sub_info.get("channel") == channel:
                try:
                    # Call resolver to transform event data
                    variables = sub_info.get("variables", {})
                    response_data = await resolver(data, variables)

                    # Send to client
                    if self.protocol and sub_info.get("adapter"):
                        await self.protocol.send_next(
                            sub_info["adapter"],
                            sub_id,
                            response_data,
                        )

                except Exception as e:
                    logger.exception(f"Error in resolver for {sub_id}")
                    if self.protocol and sub_info.get("adapter"):
                        await self.protocol.send_error(
                            sub_info["adapter"],
                            sub_id,
                            str(e),
                        )

    async def handle_connection(
        self,
        adapter: WebSocketAdapter,
        protocol: SubscriptionProtocolHandler | None = None,
    ) -> None:
        """Handle a new WebSocket connection.

        Args:
            adapter: WebSocket adapter for the framework
            protocol: Protocol handler (creates default if None)

        Example:
            @app.websocket("/graphql")
            async def websocket_endpoint(websocket):
                adapter = FastAPIWebSocketAdapter(websocket)
                manager = SubscriptionManager()
                await manager.handle_connection(adapter)
        """
        # Accept connection
        await adapter.accept(subprotocol="graphql-transport-ws")
        logger.info("WebSocket connection accepted")

        # Initialize protocol
        self.protocol = protocol or GraphQLTransportWSProtocol()
        self.state_machine = ProtocolStateMachine()

        try:
            while adapter.is_connected:
                try:
                    # Receive message from client
                    message = await adapter.receive_json()
                    await self._handle_protocol_message(adapter, message)

                except asyncio.CancelledError:
                    break
                except Exception as e:
                    logger.exception("Error handling message")
                    if self.protocol:
                        await self.protocol.send_error(adapter, None, str(e))

        except Exception:
            logger.exception("Connection error")
        finally:
            await adapter.close()
            logger.info("WebSocket connection closed")

    async def _handle_protocol_message(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Route protocol message to appropriate handler.

        Args:
            adapter: WebSocket adapter
            message: Message from client
        """
        if not self.protocol:
            return

        msg_type = message.get("type")
        logger.debug(f"Received message: {msg_type}")

        try:
            if msg_type == "connection_init":
                await self.protocol.handle_connection_init(adapter, message)
                if self.state_machine:
                    self.state_machine.on_connection_init()

            elif msg_type == "subscribe":
                await self.protocol.handle_subscribe(adapter, message)
                if self.state_machine:
                    sub_id = message.get("id")
                    if sub_id:
                        self.state_machine.on_subscribe(sub_id)
                        # Store subscription info
                        self.subscriptions[sub_id] = {
                            "adapter": adapter,
                            "channel": message.get("payload", {}).get(
                                "channel",
                                "",
                            ),
                            "variables": message.get("payload", {}).get(
                                "variables",
                                {},
                            ),
                        }

            elif msg_type == "complete":
                await self.protocol.handle_complete(adapter, message)
                if self.state_machine:
                    sub_id = message.get("id")
                    if sub_id:
                        self.state_machine.on_complete(sub_id)
                        # Remove subscription
                        self.subscriptions.pop(sub_id, None)

            elif msg_type == "ping":
                if isinstance(self.protocol, GraphQLTransportWSProtocol):
                    await self.protocol.handle_ping(adapter)

            else:
                logger.warning(f"Unknown message type: {msg_type}")

        except Exception as e:
            logger.exception(f"Error handling {msg_type}")
            await self.protocol.send_error(
                adapter,
                message.get("id"),
                str(e),
            )

    def get_active_subscriptions_count(self) -> int:
        """Get count of active subscriptions.

        Returns:
            Number of active subscriptions
        """
        return len(self.subscriptions)

    def get_registered_resolvers(self) -> list:
        """Get list of registered event types.

        Returns:
            List of event type strings
        """
        return list(self.resolvers.keys())


# ============================================================================
# Manager Factory
# ============================================================================


def create_manager(
    config: SubscriptionConfig | None = None,
) -> SubscriptionManager:
    """Factory function to create SubscriptionManager.

    Args:
        config: Optional configuration

    Returns:
        Configured SubscriptionManager instance

    Example:
        manager = create_manager(
            config=SubscriptionConfig(
                max_subscriptions_per_connection=200,
                keep_alive_interval=20.0,
            )
        )
    """
    return SubscriptionManager(config)


__all__ = [
    "ResolverFunction",
    "SubscriptionConfig",
    "SubscriptionManager",
    "create_manager",
]
