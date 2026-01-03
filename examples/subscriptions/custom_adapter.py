"""
Custom WebSocket Adapter Template

Use this as a template to integrate GraphQL subscriptions with your framework.

Steps:
1. Create WebSocketAdapter subclass
2. Implement required methods
3. Handle subscription messages
4. Use SubscriptionManager in your framework

Example for a custom framework (not FastAPI/Starlette):
"""

from abc import ABC, abstractmethod
from typing import Callable, Optional
import asyncio
import uuid

from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs


class WebSocketAdapter(ABC):
    """Abstract base class for WebSocket integration."""

    @abstractmethod
    async def accept(self) -> None:
        """Accept the WebSocket connection."""
        pass

    @abstractmethod
    async def send_text(self, data: str) -> None:
        """Send text message to client."""
        pass

    @abstractmethod
    async def send_bytes(self, data: bytes) -> None:
        """Send binary message to client."""
        pass

    @abstractmethod
    async def receive_text(self) -> str:
        """Receive text message from client."""
        pass

    @abstractmethod
    async def close(self) -> None:
        """Close the connection."""
        pass


class GraphQLSubscriptionHandler:
    """Handle GraphQL subscriptions over WebSocket."""

    def __init__(
        self,
        websocket: WebSocketAdapter,
        manager: SubscriptionManager,
        get_resolver: Callable[[str], Callable],
        user_id: str,
        tenant_id: str,
    ):
        """
        Initialize subscription handler.

        Args:
            websocket: WebSocket adapter for your framework
            manager: SubscriptionManager instance
            get_resolver: Function to get resolver by query name
            user_id: Current user ID
            tenant_id: Current tenant ID
        """
        self.websocket = websocket
        self.manager = manager
        self.get_resolver = get_resolver
        self.user_id = user_id
        self.tenant_id = tenant_id
        self.subscriptions: dict[str, str] = {}
        self.connection_id = str(uuid.uuid4())

    async def handle_connection(self) -> None:
        """Handle WebSocket connection lifecycle."""
        await self.websocket.accept()

        try:
            while True:
                # Receive message from client
                data = await self.websocket.receive_text()

                # Parse and handle message
                import json
                message = json.loads(data)
                await self._handle_message(message)

        except Exception as e:
            print(f"Connection error: {e}")

        finally:
            # Cleanup on disconnect
            await self._cleanup()

    async def _handle_message(self, message: dict) -> None:
        """Handle incoming message."""
        message_type = message.get("type")

        if message_type == "connection_init":
            await self._handle_connection_init()

        elif message_type == "subscribe":
            await self._handle_subscribe(message)

        elif message_type == "complete":
            await self._handle_complete(message)

    async def _handle_connection_init(self) -> None:
        """Handle connection initialization."""
        import json

        await self.websocket.send_text(json.dumps({
            "type": "connection_ack"
        }))

    async def _handle_subscribe(self, message: dict) -> None:
        """Handle subscription request."""
        import json

        subscription_id = message.get("id")
        query = message["payload"]["query"]
        variables = message["payload"].get("variables", {})

        try:
            # Get resolver for this subscription
            resolver_fn = self.get_resolver(query)

            # Create subscription
            await self.manager.create_subscription(
                subscription_id=subscription_id,
                connection_id=self.connection_id,
                query=query,
                variables=variables,
                resolver_fn=resolver_fn,
                user_id=self.user_id,
                tenant_id=self.tenant_id
            )

            self.subscriptions[subscription_id] = "active"

            # Send acknowledgment
            await self.websocket.send_text(json.dumps({
                "type": "next",
                "id": subscription_id,
                "payload": {"data": {}}
            }))

            # Start polling for events
            asyncio.create_task(self._poll_events(subscription_id))

        except Exception as e:
            await self.websocket.send_text(json.dumps({
                "type": "error",
                "id": subscription_id,
                "payload": {"message": str(e)}
            }))

    async def _handle_complete(self, message: dict) -> None:
        """Handle subscription completion."""
        import json

        subscription_id = message.get("id")
        await self.manager.complete_subscription(subscription_id)
        self.subscriptions.pop(subscription_id, None)

        await self.websocket.send_text(json.dumps({
            "type": "complete",
            "id": subscription_id
        }))

    async def _poll_events(self, subscription_id: str) -> None:
        """Poll for events on a subscription."""
        try:
            while True:
                response = await self.manager.get_next_event(subscription_id)

                if response:
                    await self.websocket.send_bytes(response)
                else:
                    await asyncio.sleep(0.01)

        except Exception as e:
            print(f"Polling error: {e}")

    async def _cleanup(self) -> None:
        """Cleanup resources on disconnect."""
        for subscription_id in list(self.subscriptions.keys()):
            await self.manager.complete_subscription(subscription_id)


# ============================================================================
# Example: Custom Framework Integration
# ============================================================================

class MyCustomFrameworkWebSocket(WebSocketAdapter):
    """Adapter for your custom framework's WebSocket."""

    def __init__(self, native_websocket):
        """
        Initialize adapter.

        Args:
            native_websocket: Your framework's WebSocket object
        """
        self.ws = native_websocket

    async def accept(self) -> None:
        """Accept connection (framework-specific)."""
        await self.ws.accept()

    async def send_text(self, data: str) -> None:
        """Send text (framework-specific)."""
        await self.ws.send({"type": "websocket.send", "text": data})

    async def send_bytes(self, data: bytes) -> None:
        """Send bytes (framework-specific)."""
        await self.ws.send({"type": "websocket.send", "bytes": data})

    async def receive_text(self) -> str:
        """Receive text (framework-specific)."""
        message = await self.ws.receive()
        return message["text"]

    async def close(self) -> None:
        """Close connection (framework-specific)."""
        await self.ws.close()


# ============================================================================
# Usage Example
# ============================================================================

async def example_usage():
    """Example of how to use the adapter with your framework."""

    # 1. Create resolver mapping
    def get_resolver(query: str):
        if "user" in query:
            return user_resolver
        elif "message" in query:
            return message_resolver
        else:
            return default_resolver

    async def user_resolver(event, variables):
        return {"user": {"id": event["id"], "name": event["name"]}}

    async def message_resolver(event, variables):
        return {"message": {"text": event["text"]}}

    async def default_resolver(event, variables):
        return {"data": event}

    # 2. Initialize manager
    config = _fraiseql_rs.PyEventBusConfig.memory()
    manager = SubscriptionManager(config)

    # 3. Create adapter for your WebSocket
    # (In your actual framework handler)
    # websocket = MyCustomFrameworkWebSocket(native_ws)

    # 4. Create handler
    # handler = GraphQLSubscriptionHandler(
    #     websocket=websocket,
    #     manager=manager,
    #     get_resolver=get_resolver,
    #     user_id="user123",
    #     tenant_id="tenant456"
    # )

    # 5. Handle connection
    # await handler.handle_connection()


# ============================================================================
# Integration Steps for Your Framework
# ============================================================================

"""
To integrate with your framework:

1. Identify your framework's WebSocket class:
   - Quart: quart.websocket.Websocket
   - Channels (Django): channels.generic.websocket.AsyncWebsocketConsumer
   - etc.

2. Create an adapter:
   class MyFrameworkWebSocket(WebSocketAdapter):
       def __init__(self, native_ws):
           self.ws = native_ws

       async def accept(self):
           await self.ws.accept()  # Your framework's call

       # ... implement other methods

3. In your WebSocket handler:
   async def websocket_handler(request, websocket):
       adapter = MyFrameworkWebSocket(websocket)

       manager = SubscriptionManager(config)

       handler = GraphQLSubscriptionHandler(
           websocket=adapter,
           manager=manager,
           get_resolver=get_my_resolver,
           user_id=request.user.id,
           tenant_id=request.user.tenant_id
       )

       await handler.handle_connection()

4. Define resolver mapping for your queries:
   def get_my_resolver(query: str):
       if "users" in query:
           return users_resolver
       elif "messages" in query:
           return messages_resolver
       # etc.

That's it! Your framework now supports GraphQL subscriptions.
"""
