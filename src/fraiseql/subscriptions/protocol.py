"""GraphQL Transport WebSocket Protocol Handler.

Implements the graphql-transport-ws protocol specification for GraphQL subscriptions.

Protocol flow:
1. Client → connection_init
2. Server → connection_ack
3. Client → subscribe (with query/variables)
4. Server → next (with data) or error
5. Client → complete or Server → complete
6. Connection closes

Reference: https://github.com/enisdenjo/graphql-ws/blob/master/PROTOCOL.md
"""

import asyncio  # noqa: TC003
import logging
from enum import Enum
from typing import Any

from fraiseql.subscriptions.http_adapter import (
    SubscriptionProtocolHandler,
    WebSocketAdapter,
)

logger = logging.getLogger(__name__)


# ============================================================================
# Message Type Constants
# ============================================================================


class MessageType:
    """GraphQL Transport WebSocket message types."""

    # Client → Server
    CONNECTION_INIT = "connection_init"
    SUBSCRIBE = "subscribe"
    COMPLETE = "complete"
    PING = "ping"

    # Server → Client
    CONNECTION_ACK = "connection_ack"
    CONNECTION_ERROR = "connection_error"
    NEXT = "next"
    ERROR = "error"
    COMPLETE_SERVER = "complete"
    PONG = "pong"


# ============================================================================
# Connection State Machine
# ============================================================================


class ConnectionState(Enum):
    """WebSocket connection states."""

    CONNECTING = "connecting"
    CONNECTED = "connected"
    READY = "ready"
    CLOSING = "closing"
    CLOSED = "closed"


# ============================================================================
# Message Validation
# ============================================================================


def validate_connection_init(message: dict[str, Any]) -> bool:
    """Validate connection_init message format.

    Args:
        message: Message dict to validate

    Returns:
        True if valid

    Raises:
        ValueError: If validation fails
    """
    if message.get("type") != MessageType.CONNECTION_INIT:
        raise ValueError("Expected connection_init message")
    # payload is optional
    return True


def validate_subscribe(message: dict[str, Any]) -> bool:
    """Validate subscribe message format.

    Args:
        message: Message dict to validate

    Returns:
        True if valid

    Raises:
        ValueError: If validation fails
    """
    if message.get("type") != MessageType.SUBSCRIBE:
        raise ValueError("Expected subscribe message")

    if not message.get("id"):
        raise ValueError("Subscribe message must have id")

    payload = message.get("payload", {})
    if not payload.get("query"):
        raise ValueError("Subscribe payload must have query")

    return True


def validate_complete(message: dict[str, Any]) -> bool:
    """Validate complete message format.

    Args:
        message: Message dict to validate

    Returns:
        True if valid

    Raises:
        ValueError: If validation fails
    """
    if message.get("type") != MessageType.COMPLETE:
        raise ValueError("Expected complete message")

    if not message.get("id"):
        raise ValueError("Complete message must have id")

    return True


# ============================================================================
# Message Builders
# ============================================================================


def build_connection_ack(
    payload: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build connection_ack message.

    Args:
        payload: Optional payload with server info

    Returns:
        connection_ack message dict
    """
    return {
        "type": MessageType.CONNECTION_ACK,
        "payload": payload or {},
    }


def build_connection_error(error_message: str) -> dict[str, Any]:
    """Build connection_error message.

    Args:
        error_message: Error description

    Returns:
        connection_error message dict
    """
    return {
        "type": MessageType.CONNECTION_ERROR,
        "payload": {
            "message": error_message,
        },
    }


def build_next(subscription_id: str, data: dict[str, Any]) -> dict[str, Any]:
    """Build next message with subscription data.

    Args:
        subscription_id: Subscription identifier
        data: Response data

    Returns:
        next message dict
    """
    return {
        "type": MessageType.NEXT,
        "id": subscription_id,
        "payload": {
            "data": data,
        },
    }


def build_error(
    subscription_id: str | None,
    error_message: str,
    error_code: str | None = None,
) -> dict[str, Any]:
    """Build error message.

    Args:
        subscription_id: Subscription ID (None for connection errors)
        error_message: Error description
        error_code: Optional error code (e.g., "GRAPHQL_ERROR")

    Returns:
        error message dict
    """
    message: dict[str, Any] = {
        "type": MessageType.ERROR,
        "payload": [
            {
                "message": error_message,
            },
        ],
    }

    if error_code:
        message["payload"][0]["extensions"] = {"code": error_code}

    if subscription_id:
        message["id"] = subscription_id

    return message


def build_complete(subscription_id: str) -> dict[str, Any]:
    """Build complete message.

    Args:
        subscription_id: Subscription identifier

    Returns:
        complete message dict
    """
    return {
        "type": MessageType.COMPLETE,
        "id": subscription_id,
    }


def build_pong() -> dict[str, Any]:
    """Build pong message for ping/pong keep-alive.

    Returns:
        pong message dict
    """
    return {
        "type": MessageType.PONG,
    }


# ============================================================================
# Protocol Handler Implementation
# ============================================================================


class GraphQLTransportWSProtocol(SubscriptionProtocolHandler):
    """GraphQL Transport WebSocket protocol handler.

    Handles the complete protocol lifecycle for GraphQL subscriptions
    including:
    - Connection initialization
    - Subscription creation
    - Message streaming
    - Error handling
    - Graceful shutdown
    """

    def __init__(self) -> None:
        """Initialize protocol handler."""
        self.state = ConnectionState.CONNECTING
        self.subscriptions: dict[str, Any] = {}
        self._keep_alive_task: asyncio.Task | None = None

    async def handle_connection_init(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Handle connection_init message.

        Args:
            adapter: WebSocket adapter
            message: connection_init message

        Raises:
            ValueError: If message is invalid
        """
        validate_connection_init(message)
        logger.info("Received connection_init")

        # Extract auth parameters if provided
        payload = message.get("payload", {})
        self.connection_params = payload

        # Send connection_ack
        ack_message = build_connection_ack()
        await adapter.send_json(ack_message)

        self.state = ConnectionState.READY
        logger.info("Sent connection_ack, connection ready")

    async def handle_subscribe(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Handle subscribe message.

        Args:
            adapter: WebSocket adapter
            message: subscribe message with query/variables

        Raises:
            ValueError: If message is invalid
        """
        validate_subscribe(message)

        subscription_id = message["id"]
        payload = message["payload"]

        logger.info(f"Received subscribe for subscription {subscription_id}")

        # Extract subscription details
        query = payload.get("query")
        operation_name = payload.get("operationName")
        variables = payload.get("variables", {})

        # Store subscription info
        self.subscriptions[subscription_id] = {
            "query": query,
            "operation_name": operation_name,
            "variables": variables,
            "adapter": adapter,
        }

        logger.debug(f"Subscription {subscription_id} registered")

        # Note: Actual subscription creation and event handling
        # will be done by the SubscriptionManager that uses this protocol handler

    async def handle_complete(
        self,
        adapter: WebSocketAdapter,
        message: dict[str, Any],
    ) -> None:
        """Handle complete message from client.

        Args:
            adapter: WebSocket adapter
            message: complete message

        Raises:
            ValueError: If message is invalid
        """
        validate_complete(message)

        subscription_id = message["id"]
        logger.info(f"Received complete for subscription {subscription_id}")

        # Remove subscription
        if subscription_id in self.subscriptions:
            del self.subscriptions[subscription_id]
            logger.debug(f"Subscription {subscription_id} removed")

    async def send_next(
        self,
        adapter: WebSocketAdapter,
        subscription_id: str,
        data: dict[str, Any],
    ) -> None:
        """Send next message with subscription data.

        Args:
            adapter: WebSocket adapter
            subscription_id: Subscription identifier
            data: Response data to send

        Raises:
            Exception: If send fails
        """
        message = build_next(subscription_id, data)
        await adapter.send_json(message)
        logger.debug(f"Sent next message for subscription {subscription_id}")

    async def send_error(
        self,
        adapter: WebSocketAdapter,
        subscription_id: str | None,
        error_message: str,
    ) -> None:
        """Send error message to client.

        Args:
            adapter: WebSocket adapter
            subscription_id: Subscription ID (None for connection errors)
            error_message: Error description

        Raises:
            Exception: If send fails
        """
        message = build_error(subscription_id, error_message, "GRAPHQL_ERROR")
        await adapter.send_json(message)

        if subscription_id:
            logger.warning(
                f"Sent error for subscription {subscription_id}: {error_message}",
            )
        else:
            logger.error(f"Connection error: {error_message}")

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
        message = build_complete(subscription_id)
        await adapter.send_json(message)
        logger.debug(f"Sent complete message for subscription {subscription_id}")

    async def handle_ping(self, adapter: WebSocketAdapter) -> None:
        """Handle ping message (keep-alive).

        Args:
            adapter: WebSocket adapter
        """
        logger.debug("Received ping")
        pong = build_pong()
        await adapter.send_json(pong)

    async def send_connection_error(
        self,
        adapter: WebSocketAdapter,
        error_message: str,
    ) -> None:
        """Send connection error and close.

        Args:
            adapter: WebSocket adapter
            error_message: Error description
        """
        message = build_connection_error(error_message)
        await adapter.send_json(message)
        await adapter.close(code=1000, reason=error_message)
        self.state = ConnectionState.CLOSED
        logger.error(f"Connection closed with error: {error_message}")

    async def cleanup(self) -> None:
        """Clean up protocol state on close."""
        self.state = ConnectionState.CLOSED
        self.subscriptions.clear()
        logger.info("Protocol handler cleaned up")


# ============================================================================
# State Machine Validator
# ============================================================================


class ProtocolStateMachine:
    """Validates valid message sequences for the protocol.

    Ensures clients follow the protocol correctly:
    - connection_init must be first message
    - Can't send subscribe before init
    - Can't complete non-existent subscriptions
    """

    def __init__(self) -> None:
        """Initialize state machine."""
        self.state = ConnectionState.CONNECTING
        self.active_subscriptions: set = set()

    def on_connection_init(self) -> None:
        """Process connection_init message."""
        if self.state != ConnectionState.CONNECTING:
            raise ValueError(
                "connection_init must be the first message",
            )
        self.state = ConnectionState.READY

    def on_subscribe(self, subscription_id: str) -> None:
        """Process subscribe message."""
        if self.state != ConnectionState.READY:
            raise ValueError(
                "Must send connection_init before subscribe",
            )
        if subscription_id in self.active_subscriptions:
            raise ValueError(
                f"Subscription {subscription_id} already exists",
            )
        self.active_subscriptions.add(subscription_id)

    def on_complete(self, subscription_id: str) -> None:
        """Process complete message."""
        if subscription_id not in self.active_subscriptions:
            raise ValueError(
                f"Subscription {subscription_id} not found",
            )
        self.active_subscriptions.discard(subscription_id)

    def is_valid_transition(
        self,
        current_state: str,
        message_type: str,
    ) -> bool:
        """Check if message type is valid in current state.

        Args:
            current_state: Current connection state
            message_type: Type of message being sent

        Returns:
            True if transition is valid
        """
        if current_state == "connecting":
            return message_type == MessageType.CONNECTION_INIT

        if current_state == "ready":
            return message_type in (
                MessageType.SUBSCRIBE,
                MessageType.COMPLETE,
                MessageType.PING,
            )

        return False


__all__ = [
    "ConnectionState",
    "GraphQLTransportWSProtocol",
    "MessageType",
    "ProtocolStateMachine",
    "build_complete",
    "build_connection_ack",
    "build_connection_error",
    "build_error",
    "build_next",
    "build_pong",
]
