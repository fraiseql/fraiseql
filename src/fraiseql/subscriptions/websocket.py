"""WebSocket connection handling for GraphQL subscriptions."""

import asyncio
import json
import logging
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Dict, Any, Optional, Set, AsyncIterator
from uuid import uuid4
from dataclasses import dataclass, field

from graphql import GraphQLSchema, subscribe, parse, DocumentNode
from graphql.execution import ExecutionResult

from fraiseql.core.exceptions import WebSocketError

logger = logging.getLogger(__name__)


class ConnectionState(Enum):
    """WebSocket connection states."""
    CONNECTING = "connecting"
    READY = "ready"
    CLOSING = "closing"
    CLOSED = "closed"


class SubProtocol(Enum):
    """Supported WebSocket subprotocols."""
    GRAPHQL_WS = "graphql-ws"  # Legacy Apollo protocol
    GRAPHQL_TRANSPORT_WS = "graphql-transport-ws"  # New protocol


class MessageType:
    """GraphQL WebSocket message types."""
    # Client -> Server
    CONNECTION_INIT = "connection_init"
    CONNECTION_TERMINATE = "connection_terminate"  # Legacy
    SUBSCRIBE = "subscribe"
    COMPLETE = "complete"
    PING = "ping"
    
    # Server -> Client
    CONNECTION_ACK = "connection_ack"
    CONNECTION_ERROR = "connection_error"
    NEXT = "next"  # graphql-transport-ws
    DATA = "data"  # graphql-ws (legacy)
    ERROR = "error"
    COMPLETE_SERVER = "complete"
    PONG = "pong"
    
    # Aliases for compatibility
    START = "start"  # Legacy alias for subscribe
    STOP = "stop"    # Legacy alias for complete


@dataclass
class GraphQLWSMessage:
    """GraphQL WebSocket message."""
    type: str
    id: Optional[str] = None
    payload: Optional[Dict[str, Any]] = None
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for sending."""
        result = {"type": self.type}
        if self.id is not None:
            result["id"] = self.id
        if self.payload is not None:
            result["payload"] = self.payload
        return result
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GraphQLWSMessage":
        """Create from received dictionary."""
        msg_type = data.get("type")
        if not msg_type:
            raise ValueError("Message type is required")
        
        # Handle legacy message types
        if msg_type == MessageType.START:
            msg_type = MessageType.SUBSCRIBE
        elif msg_type == MessageType.STOP:
            msg_type = MessageType.COMPLETE
        
        return cls(
            type=msg_type,
            id=data.get("id"),
            payload=data.get("payload")
        )


class WebSocketConnection:
    """Manages a single WebSocket connection."""
    
    def __init__(
        self,
        websocket: Any,
        connection_id: Optional[str] = None,
        subprotocol: SubProtocol = SubProtocol.GRAPHQL_WS,
        connection_init_timeout: float = 10.0,
        keep_alive_interval: float = 30.0
    ):
        self.websocket = websocket
        self.connection_id = connection_id or str(uuid4())
        self.subprotocol = subprotocol
        self.connection_init_timeout = connection_init_timeout
        self.keep_alive_interval = keep_alive_interval
        
        self.state = ConnectionState.CONNECTING
        self.schema: Optional[GraphQLSchema] = None
        self.context: Dict[str, Any] = {}
        self.subscriptions: Dict[str, asyncio.Task] = {}
        self.connection_params: Optional[Dict[str, Any]] = None
        self.initialized_at: Optional[datetime] = None
        
        self._keep_alive_task: Optional[asyncio.Task] = None
        self._message_queue: asyncio.Queue = asyncio.Queue()
    
    async def handle(self):
        """Handle the WebSocket connection lifecycle."""
        try:
            # Wait for connection_init
            await self._wait_for_connection_init()
            
            # Start keep-alive if needed
            if self.keep_alive_interval > 0:
                self._keep_alive_task = asyncio.create_task(self._keep_alive())
            
            # Main message loop
            await self._message_loop()
            
        except asyncio.CancelledError:
            logger.info(f"Connection {self.connection_id} cancelled")
        except Exception as e:
            logger.error(f"Connection {self.connection_id} error: {e}")
            await self._send_error(None, str(e))
        finally:
            await self._cleanup()
    
    async def _wait_for_connection_init(self):
        """Wait for connection_init message."""
        timeout = self.connection_init_timeout
        deadline = asyncio.get_event_loop().time() + timeout
        
        while asyncio.get_event_loop().time() < deadline:
            try:
                # Wait for message with timeout
                remaining = deadline - asyncio.get_event_loop().time()
                message = await asyncio.wait_for(
                    self._receive_message(),
                    timeout=remaining
                )
                
                if message.type == MessageType.CONNECTION_INIT:
                    self.connection_params = message.payload or {}
                    self.initialized_at = datetime.now(timezone.utc)
                    
                    # Send connection_ack
                    await self.send_message(GraphQLWSMessage(
                        type=MessageType.CONNECTION_ACK
                    ))
                    
                    self.state = ConnectionState.READY
                    logger.info(f"Connection {self.connection_id} initialized")
                    return
                else:
                    # Unexpected message before init
                    await self._close(
                        code=4400,
                        reason="Connection initialisation must be first message"
                    )
                    return
                    
            except asyncio.TimeoutError:
                await self._close(
                    code=4408,
                    reason="Connection initialisation timeout"
                )
                raise
    
    async def _message_loop(self):
        """Main message processing loop."""
        while self.state == ConnectionState.READY:
            try:
                message = await self._receive_message()
                await self._handle_message(message)
            except asyncio.CancelledError:
                raise
            except Exception as e:
                if "disconnect" in str(e).lower():
                    # Normal disconnect
                    break
                logger.error(f"Message handling error: {e}")
                await self._send_error(None, str(e))
    
    async def _receive_message(self) -> GraphQLWSMessage:
        """Receive and parse a message."""
        raw_message = await self.websocket.receive()
        
        # Handle disconnect
        if raw_message.get("type") == "websocket.disconnect":
            self.state = ConnectionState.CLOSING
            raise WebSocketError("Client disconnected")
        
        # Parse message
        text = raw_message.get("text", "")
        try:
            data = json.loads(text)
            return GraphQLWSMessage.from_dict(data)
        except (json.JSONDecodeError, ValueError) as e:
            raise WebSocketError(f"Invalid message format: {e}")
    
    async def send_message(self, message: GraphQLWSMessage):
        """Send a message to the client."""
        if self.state not in (ConnectionState.READY, ConnectionState.CONNECTING):
            return
        
        try:
            await self.websocket.send(json.dumps(message.to_dict()))
        except Exception as e:
            logger.error(f"Failed to send message: {e}")
            self.state = ConnectionState.CLOSING
            raise
    
    async def _handle_message(self, message: GraphQLWSMessage):
        """Handle incoming message based on type."""
        handlers = {
            MessageType.SUBSCRIBE: self._handle_subscribe,
            MessageType.COMPLETE: self._handle_complete,
            MessageType.CONNECTION_TERMINATE: self._handle_terminate,
            MessageType.PING: self._handle_ping,
        }
        
        handler = handlers.get(message.type)
        if handler:
            await handler(message)
        else:
            logger.warning(f"Unknown message type: {message.type}")
    
    async def _handle_subscribe(self, message: GraphQLWSMessage):
        """Handle subscription request."""
        if not message.id:
            await self._send_error(None, "Subscription ID is required")
            return
        
        if message.id in self.subscriptions:
            await self._send_error(
                message.id,
                f"Subscription {message.id} already exists"
            )
            return
        
        try:
            # Parse query
            query = message.payload.get("query", "")
            variables = message.payload.get("variables", {})
            operation_name = message.payload.get("operationName")
            
            document = parse(query)
            
            # Execute subscription
            result = await subscribe(
                self.schema,
                document,
                root_value=None,
                context_value=self.context,
                variable_values=variables,
                operation_name=operation_name
            )
            
            if isinstance(result, AsyncIterator):
                # Start subscription task
                task = asyncio.create_task(
                    self._handle_subscription_generator(message.id, result)
                )
                self.subscriptions[message.id] = task
            else:
                # Single error result
                await self._send_error(message.id, result)
                
        except Exception as e:
            logger.error(f"Subscription error: {e}")
            await self._send_error(message.id, str(e))
    
    async def _handle_subscription_generator(
        self,
        subscription_id: str,
        result_iterator: AsyncIterator[ExecutionResult]
    ):
        """Handle subscription result generator."""
        try:
            async for result in result_iterator:
                if result.errors:
                    await self._send_error(subscription_id, result.errors)
                else:
                    # Send data
                    msg_type = (MessageType.NEXT 
                               if self.subprotocol == SubProtocol.GRAPHQL_TRANSPORT_WS
                               else MessageType.DATA)
                    
                    await self.send_message(GraphQLWSMessage(
                        type=msg_type,
                        id=subscription_id,
                        payload={"data": result.data}
                    ))
            
            # Send complete
            await self.send_message(GraphQLWSMessage(
                type=MessageType.COMPLETE_SERVER,
                id=subscription_id
            ))
            
        except asyncio.CancelledError:
            raise
        except Exception as e:
            logger.error(f"Subscription {subscription_id} error: {e}")
            await self._send_error(subscription_id, str(e))
        finally:
            # Clean up
            self.subscriptions.pop(subscription_id, None)
    
    async def _handle_complete(self, message: GraphQLWSMessage):
        """Handle subscription completion request."""
        if message.id and message.id in self.subscriptions:
            task = self.subscriptions.pop(message.id)
            task.cancel()
            logger.info(f"Subscription {message.id} completed")
    
    async def _handle_terminate(self, message: GraphQLWSMessage):
        """Handle connection termination request."""
        self.state = ConnectionState.CLOSING
        await self._close(code=1000, reason="Client requested termination")
    
    async def _handle_ping(self, message: GraphQLWSMessage):
        """Handle ping message."""
        await self.send_message(GraphQLWSMessage(
            type=MessageType.PONG,
            payload=message.payload
        ))
    
    async def _send_error(self, subscription_id: Optional[str], error: Any):
        """Send error message."""
        payload = {"errors": [str(error)]} if isinstance(error, str) else error
        
        await self.send_message(GraphQLWSMessage(
            type=MessageType.ERROR,
            id=subscription_id,
            payload=payload
        ))
    
    async def _keep_alive(self):
        """Send periodic keep-alive pings."""
        while self.state == ConnectionState.READY:
            try:
                await asyncio.sleep(self.keep_alive_interval)
                
                # Send ping
                await self.send_message(GraphQLWSMessage(
                    type=MessageType.PING,
                    payload={"timestamp": datetime.now(timezone.utc).isoformat()}
                ))
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Keep-alive error: {e}")
                break
    
    async def _close(self, code: int = 1000, reason: str = ""):
        """Close the WebSocket connection."""
        if self.state == ConnectionState.CLOSED:
            return
        
        self.state = ConnectionState.CLOSING
        
        try:
            await self.websocket.close(code=code, reason=reason)
        except Exception as e:
            logger.error(f"Error closing WebSocket: {e}")
        
        self.state = ConnectionState.CLOSED
    
    async def _cleanup(self):
        """Clean up resources."""
        # Cancel keep-alive
        if self._keep_alive_task:
            self._keep_alive_task.cancel()
        
        # Cancel all subscriptions
        for task in self.subscriptions.values():
            task.cancel()
        
        # Wait for cancellations
        if self.subscriptions:
            await asyncio.gather(
                *self.subscriptions.values(),
                return_exceptions=True
            )
        
        self.subscriptions.clear()
        self.state = ConnectionState.CLOSED
        
        logger.info(f"Connection {self.connection_id} cleaned up")


class SubscriptionManager:
    """Manages all WebSocket subscription connections."""
    
    def __init__(self):
        self.connections: Dict[str, WebSocketConnection] = {}
        self.schema: Optional[GraphQLSchema] = None
        self._lock = asyncio.Lock()
    
    async def add_connection(
        self,
        websocket: Any,
        subprotocol: Optional[str] = None,
        context: Optional[Dict[str, Any]] = None
    ) -> WebSocketConnection:
        """Add a new WebSocket connection."""
        # Determine subprotocol
        if subprotocol == "graphql-transport-ws":
            protocol = SubProtocol.GRAPHQL_TRANSPORT_WS
        else:
            protocol = SubProtocol.GRAPHQL_WS
        
        # Create connection
        connection = WebSocketConnection(
            websocket=websocket,
            subprotocol=protocol
        )
        
        # Set schema and context
        connection.schema = self.schema
        connection.context = context or {}
        
        # Register connection
        async with self._lock:
            self.connections[connection.connection_id] = connection
        
        logger.info(f"Added connection {connection.connection_id}")
        return connection
    
    async def remove_connection(self, connection_id: str):
        """Remove a connection."""
        async with self._lock:
            connection = self.connections.pop(connection_id, None)
        
        if connection:
            await connection._cleanup()
            logger.info(f"Removed connection {connection_id}")
    
    async def broadcast(
        self,
        message: GraphQLWSMessage,
        subscription_id: Optional[str] = None,
        filter_fn: Optional[Any] = None
    ):
        """Broadcast message to all connections."""
        # Get active connections
        async with self._lock:
            connections = list(self.connections.values())
        
        # Send to each connection
        tasks = []
        for conn in connections:
            if conn.state != ConnectionState.READY:
                continue
            
            if filter_fn and not filter_fn(conn):
                continue
            
            if subscription_id and subscription_id not in conn.subscriptions:
                continue
            
            tasks.append(conn.send_message(message))
        
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)
    
    async def close_all(self):
        """Close all connections."""
        async with self._lock:
            connections = list(self.connections.values())
            self.connections.clear()
        
        # Close all connections
        tasks = [conn._close() for conn in connections]
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)
        
        logger.info("Closed all connections")