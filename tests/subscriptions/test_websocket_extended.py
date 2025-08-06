"""Extended tests for WebSocket subscription handling to improve coverage."""

import asyncio
import json
from unittest.mock import AsyncMock, Mock, patch

import pytest
from graphql import ExecutionResult, build_schema

from fraiseql.core.exceptions import WebSocketError
from fraiseql.subscriptions.websocket import (
    ConnectionState,
    GraphQLWSMessage,
    MessageType,
    SubProtocol,
    SubscriptionManager,
    WebSocketConnection,
)


class TestGraphQLWSMessage:
    """Test GraphQLWSMessage class."""

    def test_message_creation(self):
        """Test creating GraphQL WebSocket messages."""
        message = GraphQLWSMessage(
            type=MessageType.SUBSCRIBE, id="sub1", payload={"query": "subscription { test }"}
        )

        assert message.type == MessageType.SUBSCRIBE
        assert message.id == "sub1"
        assert message.payload["query"] == "subscription { test }"

    def test_message_to_dict(self):
        """Test converting message to dictionary."""
        message = GraphQLWSMessage(
            type=MessageType.CONNECTION_ACK, id="test_id", payload={"data": "test"}
        )

        result = message.to_dict()
        expected = {
            "type": MessageType.CONNECTION_ACK,
            "id": "test_id",
            "payload": {"data": "test"},
        }
        assert result == expected

    def test_message_to_dict_minimal(self):
        """Test converting minimal message to dictionary."""
        message = GraphQLWSMessage(type=MessageType.CONNECTION_INIT)

        result = message.to_dict()
        expected = {"type": MessageType.CONNECTION_INIT}
        assert result == expected

    def test_message_from_dict(self):
        """Test creating message from dictionary."""
        data = {
            "type": MessageType.SUBSCRIBE,
            "id": "sub1",
            "payload": {"query": "subscription { test }"},
        }

        message = GraphQLWSMessage.from_dict(data)
        assert message.type == MessageType.SUBSCRIBE
        assert message.id == "sub1"
        assert message.payload == {"query": "subscription { test }"}

    def test_message_from_dict_legacy_types(self):
        """Test creating message from dictionary with legacy types."""
        # Test START -> SUBSCRIBE conversion
        start_data = {"type": MessageType.START, "id": "sub1"}
        message = GraphQLWSMessage.from_dict(start_data)
        assert message.type == MessageType.SUBSCRIBE

        # Test STOP -> COMPLETE conversion
        stop_data = {"type": MessageType.STOP, "id": "sub1"}
        message = GraphQLWSMessage.from_dict(stop_data)
        assert message.type == MessageType.COMPLETE

    def test_message_from_dict_no_type(self):
        """Test creating message from dictionary without type."""
        with pytest.raises(ValueError, match="Message type is required"):
            GraphQLWSMessage.from_dict({})

    def test_message_from_dict_minimal(self):
        """Test creating minimal message from dictionary."""
        data = {"type": MessageType.CONNECTION_INIT}
        message = GraphQLWSMessage.from_dict(data)

        assert message.type == MessageType.CONNECTION_INIT
        assert message.id is None
        assert message.payload is None


class TestWebSocketConnection:
    """Test WebSocketConnection class."""

    def setup_method(self):
        """Set up test fixtures."""
        self.mock_websocket = AsyncMock()
        self.schema = build_schema(
            """type Query { hello: String } type Subscription { test: String }"""
        )

    def test_connection_initialization(self):
        """Test WebSocket connection initialization."""
        connection = WebSocketConnection(
            websocket=self.mock_websocket,
            connection_id="test_conn",
            subprotocol=SubProtocol.GRAPHQL_WS,
            connection_init_timeout=5.0,
            keep_alive_interval=15.0,
        )

        assert connection.websocket is self.mock_websocket
        assert connection.connection_id == "test_conn"
        assert connection.subprotocol == SubProtocol.GRAPHQL_WS
        assert connection.connection_init_timeout == 5.0
        assert connection.keep_alive_interval == 15.0
        assert connection.state == ConnectionState.CONNECTING

    def test_connection_auto_id_generation(self):
        """Test automatic connection ID generation."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        assert connection.connection_id is not None
        assert len(connection.connection_id) > 0

    @pytest.mark.asyncio
    async def test_send_message(self):
        """Test sending message through WebSocket."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.READY

        message = GraphQLWSMessage(type=MessageType.CONNECTION_ACK)
        await connection.send_message(message)

        expected_json = json.dumps({"type": MessageType.CONNECTION_ACK})
        self.mock_websocket.send.assert_called_once_with(expected_json)

    @pytest.mark.asyncio
    async def test_send_message_connection_not_ready(self):
        """Test sending message when connection is not ready."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.CLOSED

        message = GraphQLWSMessage(type=MessageType.CONNECTION_ACK)
        await connection.send_message(message)

        # Should not send anything
        self.mock_websocket.send.assert_not_called()

    @pytest.mark.asyncio
    async def test_send_message_exception(self):
        """Test sending message with WebSocket exception."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.READY

        self.mock_websocket.send.side_effect = Exception("Send failed")

        message = GraphQLWSMessage(type=MessageType.CONNECTION_ACK)
        with pytest.raises(Exception, match="Send failed"):
            await connection.send_message(message)

        assert connection.state == ConnectionState.CLOSING

    @pytest.mark.asyncio
    async def test_receive_message(self):
        """Test receiving and parsing WebSocket message."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        raw_message = {
            "text": json.dumps({"type": MessageType.CONNECTION_INIT, "payload": {"auth": "token"}})
        }
        self.mock_websocket.receive.return_value = raw_message

        message = await connection._receive_message()
        assert message.type == MessageType.CONNECTION_INIT
        assert message.payload == {"auth": "token"}

    @pytest.mark.asyncio
    async def test_receive_message_disconnect(self):
        """Test receiving disconnect message."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        self.mock_websocket.receive.return_value = {"type": "websocket.disconnect"}

        with pytest.raises(WebSocketError, match="Client disconnected"):
            await connection._receive_message()

        assert connection.state == ConnectionState.CLOSING

    @pytest.mark.asyncio
    async def test_receive_message_invalid_json(self):
        """Test receiving invalid JSON message."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        self.mock_websocket.receive.return_value = {"text": "invalid json"}

        with pytest.raises(WebSocketError, match="Invalid message format"):
            await connection._receive_message()

    @pytest.mark.asyncio
    async def test_wait_for_connection_init_success(self):
        """Test successful connection initialization."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        init_message = GraphQLWSMessage(
            type=MessageType.CONNECTION_INIT, payload={"authorization": "Bearer token"}
        )

        # Mock receiving the init message
        self.mock_websocket.receive.return_value = {"text": json.dumps(init_message.to_dict())}

        await connection._wait_for_connection_init()

        assert connection.state == ConnectionState.READY
        assert connection.connection_params == {"authorization": "Bearer token"}
        assert connection.initialized_at is not None

        # Should send ACK
        expected_ack = json.dumps({"type": MessageType.CONNECTION_ACK})
        self.mock_websocket.send.assert_called_with(expected_ack)

    @pytest.mark.asyncio
    async def test_wait_for_connection_init_timeout(self):
        """Test connection initialization timeout."""
        connection = WebSocketConnection(
            websocket=self.mock_websocket,
            connection_init_timeout=0.1,  # Short timeout for testing
        )

        # Make receive hang
        async def hang_forever():
            await asyncio.sleep(10)  # Much longer than timeout

        self.mock_websocket.receive.side_effect = hang_forever

        with pytest.raises(TimeoutError):
            await connection._wait_for_connection_init()

    @pytest.mark.asyncio
    async def test_wait_for_connection_init_wrong_message(self):
        """Test receiving wrong message before init."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        # Send subscribe before init
        wrong_message = GraphQLWSMessage(type=MessageType.SUBSCRIBE, id="sub1")
        self.mock_websocket.receive.return_value = {"text": json.dumps(wrong_message.to_dict())}

        with patch.object(connection, "_close") as mock_close:
            await connection._wait_for_connection_init()
            mock_close.assert_called_once_with(
                code=4400, reason="Connection initialisation must be first message"
            )

    @pytest.mark.asyncio
    async def test_handle_subscribe_success(self):
        """Test handling successful subscription."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.schema = self.schema
        connection.state = ConnectionState.READY

        message = GraphQLWSMessage(
            type=MessageType.SUBSCRIBE,
            id="sub1",
            payload={"query": "subscription { test }", "variables": {}, "operationName": None},
        )

        # Mock subscription result
        async def mock_subscription():
            yield ExecutionResult(data={"test": "value1"})
            yield ExecutionResult(data={"test": "value2"})

        with patch("fraiseql.subscriptions.websocket.subscribe", return_value=mock_subscription()):
            await connection._handle_subscribe(message)

        # Should create subscription task
        assert "sub1" in connection.subscriptions
        assert isinstance(connection.subscriptions["sub1"], asyncio.Task)

    @pytest.mark.asyncio
    async def test_handle_subscribe_no_id(self):
        """Test handling subscription without ID."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        message = GraphQLWSMessage(type=MessageType.SUBSCRIBE)

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection._handle_subscribe(message)
            mock_send_error.assert_called_once_with(None, "Subscription ID is required")

    @pytest.mark.asyncio
    async def test_handle_subscribe_duplicate_id(self):
        """Test handling subscription with duplicate ID."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.subscriptions["sub1"] = Mock()  # Existing subscription

        message = GraphQLWSMessage(type=MessageType.SUBSCRIBE, id="sub1")

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection._handle_subscribe(message)
            mock_send_error.assert_called_once_with("sub1", "Subscription sub1 already exists")

    @pytest.mark.asyncio
    async def test_handle_subscribe_parse_error(self):
        """Test handling subscription with parse error."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.schema = self.schema

        message = GraphQLWSMessage(
            type=MessageType.SUBSCRIBE, id="sub1", payload={"query": "invalid query syntax"}
        )

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection._handle_subscribe(message)
            mock_send_error.assert_called_once()

    @pytest.mark.asyncio
    async def test_handle_subscription_generator_success(self):
        """Test handling subscription generator with successful results."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.subprotocol = SubProtocol.GRAPHQL_TRANSPORT_WS
        connection.state = ConnectionState.READY

        async def mock_results():
            yield ExecutionResult(data={"test": "value1"})
            yield ExecutionResult(data={"test": "value2"})

        with patch.object(connection, "send_message") as mock_send:
            await connection._handle_subscription_generator("sub1", mock_results())

        # Should send data messages and complete
        assert mock_send.call_count == 3  # 2 data + 1 complete

        # Check message types
        calls = mock_send.call_args_list
        assert calls[0][0][0].type == MessageType.NEXT  # Transport WS protocol
        assert calls[1][0][0].type == MessageType.NEXT
        assert calls[2][0][0].type == MessageType.COMPLETE_SERVER

    @pytest.mark.asyncio
    async def test_handle_subscription_generator_legacy_protocol(self):
        """Test handling subscription with legacy protocol."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.subprotocol = SubProtocol.GRAPHQL_WS  # Legacy protocol
        connection.state = ConnectionState.READY

        async def mock_results():
            yield ExecutionResult(data={"test": "value"})

        with patch.object(connection, "send_message") as mock_send:
            await connection._handle_subscription_generator("sub1", mock_results())

        # Should use DATA message type for legacy protocol
        calls = mock_send.call_args_list
        assert calls[0][0][0].type == MessageType.DATA

    @pytest.mark.asyncio
    async def test_handle_subscription_generator_with_errors(self):
        """Test handling subscription generator with errors."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        async def mock_results():
            yield ExecutionResult(errors=["Test error"])

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection._handle_subscription_generator("sub1", mock_results())
            mock_send_error.assert_called_once_with("sub1", ["Test error"])

    @pytest.mark.asyncio
    async def test_handle_subscription_generator_exception(self):
        """Test handling subscription generator with exception."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        async def failing_results():
            raise ValueError("Generator failed")
            yield  # Make it a generator

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection._handle_subscription_generator("sub1", failing_results())
            mock_send_error.assert_called_once_with("sub1", "Generator failed")

    @pytest.mark.asyncio
    async def test_handle_complete(self):
        """Test handling subscription completion."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        # Create mock subscription task
        mock_task = Mock()
        connection.subscriptions["sub1"] = mock_task

        message = GraphQLWSMessage(type=MessageType.COMPLETE, id="sub1")
        await connection._handle_complete(message)

        # Should cancel and remove subscription
        mock_task.cancel.assert_called_once()
        assert "sub1" not in connection.subscriptions

    @pytest.mark.asyncio
    async def test_handle_complete_nonexistent(self):
        """Test handling completion for nonexistent subscription."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        message = GraphQLWSMessage(type=MessageType.COMPLETE, id="nonexistent")

        # Should not raise error
        await connection._handle_complete(message)

    @pytest.mark.asyncio
    async def test_handle_terminate(self):
        """Test handling connection termination."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        message = GraphQLWSMessage(type=MessageType.CONNECTION_TERMINATE)

        with patch.object(connection, "_close") as mock_close:
            await connection._handle_terminate(message)
            assert connection.state == ConnectionState.CLOSING
            mock_close.assert_called_once_with(code=1000, reason="Client requested termination")

    @pytest.mark.asyncio
    async def test_handle_ping(self):
        """Test handling ping message."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        message = GraphQLWSMessage(
            type=MessageType.PING, payload={"timestamp": "2023-01-01T00:00:00Z"}
        )

        with patch.object(connection, "send_message") as mock_send:
            await connection._handle_ping(message)

        # Should send pong with same payload
        mock_send.assert_called_once()
        pong_message = mock_send.call_args[0][0]
        assert pong_message.type == MessageType.PONG
        assert pong_message.payload == {"timestamp": "2023-01-01T00:00:00Z"}

    @pytest.mark.asyncio
    async def test_send_error_string(self):
        """Test sending error message with string error."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        with patch.object(connection, "send_message") as mock_send:
            await connection._send_error("sub1", "Test error")

        # Should send error message
        mock_send.assert_called_once()
        error_message = mock_send.call_args[0][0]
        assert error_message.type == MessageType.ERROR
        assert error_message.id == "sub1"
        assert error_message.payload == {"errors": [{"message": "Test error"}]}

    @pytest.mark.asyncio
    async def test_send_error_object(self):
        """Test sending error message with error object."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        error_obj = {"message": "Complex error", "code": "ERR_001"}

        with patch.object(connection, "send_message") as mock_send:
            await connection._send_error("sub1", error_obj)

        # Should send error message with original object
        mock_send.assert_called_once()
        error_message = mock_send.call_args[0][0]
        assert error_message.payload == error_obj

    @pytest.mark.asyncio
    async def test_keep_alive(self):
        """Test keep-alive functionality."""
        connection = WebSocketConnection(
            websocket=self.mock_websocket,
            keep_alive_interval=0.01,  # Very short interval
        )
        connection.state = ConnectionState.READY

        with patch.object(connection, "send_message") as mock_send:
            # Run keep-alive for a short time
            keep_alive_task = asyncio.create_task(connection._keep_alive())
            await asyncio.sleep(0.05)  # Let it send a few pings
            keep_alive_task.cancel()

            try:
                await keep_alive_task
            except asyncio.CancelledError:
                pass

        # Should have sent at least one ping
        assert mock_send.call_count >= 1
        ping_message = mock_send.call_args[0][0]
        assert ping_message.type == MessageType.PING
        assert "timestamp" in ping_message.payload

    @pytest.mark.asyncio
    async def test_cleanup(self):
        """Test connection cleanup."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        # Add mock tasks
        mock_keep_alive = Mock(spec=asyncio.Task)
        mock_keep_alive.cancel = Mock()
        mock_keep_alive.done = Mock(return_value=True)

        mock_subscription = Mock(spec=asyncio.Task)
        mock_subscription.cancel = Mock()
        mock_subscription.done = Mock(return_value=True)

        connection._keep_alive_task = mock_keep_alive
        connection.subscriptions["sub1"] = mock_subscription

        await connection._cleanup()

        # Should cancel all tasks
        mock_keep_alive.cancel.assert_called_once()
        mock_subscription.cancel.assert_called_once()

        # Should clear subscriptions
        assert len(connection.subscriptions) == 0
        assert connection.state == ConnectionState.CLOSED

    @pytest.mark.asyncio
    async def test_close(self):
        """Test closing WebSocket connection."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        await connection._close(code=1000, reason="Normal close")

        self.mock_websocket.close.assert_called_once_with(code=1000, reason="Normal close")
        assert connection.state == ConnectionState.CLOSED

    @pytest.mark.asyncio
    async def test_close_already_closed(self):
        """Test closing already closed connection."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.CLOSED

        await connection._close()

        # Should not call websocket.close
        self.mock_websocket.close.assert_not_called()

    @pytest.mark.asyncio
    async def test_close_exception(self):
        """Test closing with WebSocket exception."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        self.mock_websocket.close.side_effect = Exception("Close failed")

        # Should not raise
        await connection._close()
        assert connection.state == ConnectionState.CLOSED

    @pytest.mark.asyncio
    async def test_message_loop(self):
        """Test main message processing loop."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.READY

        # Mock receiving a ping message then disconnect
        ping_msg = GraphQLWSMessage(type=MessageType.PING)
        self.mock_websocket.receive.side_effect = [
            {"text": json.dumps(ping_msg.to_dict())},
            {"type": "websocket.disconnect"},  # Simulate disconnect
        ]

        with patch.object(connection, "_handle_message") as mock_handle:
            await connection._message_loop()

        # Should handle the ping message
        mock_handle.assert_called_once()

    @pytest.mark.asyncio
    async def test_message_loop_exception(self):
        """Test message loop with exception handling."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.READY

        # Mock exception then disconnect
        self.mock_websocket.receive.side_effect = [
            Exception("Random error"),
            {"type": "websocket.disconnect"},
        ]

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection._message_loop()
            mock_send_error.assert_called_once_with(None, "Random error")

    @pytest.mark.asyncio
    async def test_unknown_message_type(self):
        """Test handling unknown message type."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        message = GraphQLWSMessage(type="unknown_type")

        with patch("fraiseql.subscriptions.websocket.logger") as mock_logger:
            await connection._handle_message(message)
            mock_logger.warning.assert_called_once_with("Unknown message type: %s", "unknown_type")


class TestSubscriptionManager:
    """Test SubscriptionManager class."""

    def setup_method(self):
        """Set up test fixtures."""
        self.manager = SubscriptionManager()
        self.schema = build_schema(
            """type Query { hello: String } type Subscription { test: String }"""
        )
        self.manager.schema = self.schema

    @pytest.mark.asyncio
    async def test_add_connection(self):
        """Test adding a connection to the manager."""
        mock_websocket = AsyncMock()

        connection = await self.manager.add_connection(
            websocket=mock_websocket, subprotocol="graphql-transport-ws", context={"user_id": 123}
        )

        assert connection.websocket is mock_websocket
        assert connection.subprotocol == SubProtocol.GRAPHQL_TRANSPORT_WS
        assert connection.schema is self.schema
        assert connection.context == {"user_id": 123}

        # Should be registered
        assert connection.connection_id in self.manager.connections

    @pytest.mark.asyncio
    async def test_add_connection_legacy_protocol(self):
        """Test adding connection with legacy protocol."""
        mock_websocket = AsyncMock()

        connection = await self.manager.add_connection(
            websocket=mock_websocket,
            subprotocol="graphql-ws",  # Legacy protocol
        )

        assert connection.subprotocol == SubProtocol.GRAPHQL_WS

    @pytest.mark.asyncio
    async def test_add_connection_unknown_protocol(self):
        """Test adding connection with unknown protocol defaults to legacy."""
        mock_websocket = AsyncMock()

        connection = await self.manager.add_connection(
            websocket=mock_websocket, subprotocol="unknown-protocol"
        )

        assert connection.subprotocol == SubProtocol.GRAPHQL_WS

    @pytest.mark.asyncio
    async def test_remove_connection(self):
        """Test removing a connection."""
        mock_websocket = AsyncMock()
        connection = await self.manager.add_connection(websocket=mock_websocket)
        connection_id = connection.connection_id

        # Verify it's added
        assert connection_id in self.manager.connections

        # Remove it
        await self.manager.remove_connection(connection_id)

        # Verify it's removed
        assert connection_id not in self.manager.connections

    @pytest.mark.asyncio
    async def test_remove_nonexistent_connection(self):
        """Test removing a nonexistent connection."""
        # Should not raise error
        await self.manager.remove_connection("nonexistent")

    @pytest.mark.asyncio
    async def test_broadcast_all_connections(self):
        """Test broadcasting to all connections."""
        # Add multiple connections
        mock_ws1 = AsyncMock()
        mock_ws2 = AsyncMock()

        conn1 = await self.manager.add_connection(websocket=mock_ws1)
        conn2 = await self.manager.add_connection(websocket=mock_ws2)

        # Set connections to ready state
        conn1.state = ConnectionState.READY
        conn2.state = ConnectionState.READY

        message = GraphQLWSMessage(type=MessageType.PING)

        with (
            patch.object(conn1, "send_message") as mock_send1,
            patch.object(conn2, "send_message") as mock_send2,
        ):
            await self.manager.broadcast(message)

            # Both should receive the message
            mock_send1.assert_called_once_with(message)
            mock_send2.assert_called_once_with(message)

    @pytest.mark.asyncio
    async def test_broadcast_with_filter(self):
        """Test broadcasting with connection filter."""
        mock_ws1 = AsyncMock()
        mock_ws2 = AsyncMock()

        conn1 = await self.manager.add_connection(websocket=mock_ws1, context={"type": "admin"})
        conn2 = await self.manager.add_connection(websocket=mock_ws2, context={"type": "user"})

        conn1.state = ConnectionState.READY
        conn2.state = ConnectionState.READY

        # Filter for admin connections only
        def admin_filter(conn):
            return conn.context.get("type") == "admin"

        message = GraphQLWSMessage(type=MessageType.PING)

        with (
            patch.object(conn1, "send_message") as mock_send1,
            patch.object(conn2, "send_message") as mock_send2,
        ):
            await self.manager.broadcast(message, filter_fn=admin_filter)

            # Only admin connection should receive
            mock_send1.assert_called_once_with(message)
            mock_send2.assert_not_called()

    @pytest.mark.asyncio
    async def test_broadcast_with_subscription_filter(self):
        """Test broadcasting with subscription ID filter."""
        mock_ws1 = AsyncMock()
        mock_ws2 = AsyncMock()

        conn1 = await self.manager.add_connection(websocket=mock_ws1)
        conn2 = await self.manager.add_connection(websocket=mock_ws2)

        conn1.state = ConnectionState.READY
        conn2.state = ConnectionState.READY

        # Add subscription to only one connection
        conn1.subscriptions["sub1"] = Mock()

        message = GraphQLWSMessage(type=MessageType.NEXT, id="sub1")

        with (
            patch.object(conn1, "send_message") as mock_send1,
            patch.object(conn2, "send_message") as mock_send2,
        ):
            await self.manager.broadcast(message, subscription_id="sub1")

            # Only connection with subscription should receive
            mock_send1.assert_called_once_with(message)
            mock_send2.assert_not_called()

    @pytest.mark.asyncio
    async def test_broadcast_not_ready_connections(self):
        """Test broadcasting skips connections that are not ready."""
        mock_ws = AsyncMock()
        conn = await self.manager.add_connection(websocket=mock_ws)

        # Keep connection in connecting state
        conn.state = ConnectionState.CONNECTING

        message = GraphQLWSMessage(type=MessageType.PING)

        with patch.object(conn, "send_message") as mock_send:
            await self.manager.broadcast(message)

            # Should not send to non-ready connection
            mock_send.assert_not_called()

    @pytest.mark.asyncio
    async def test_broadcast_no_connections(self):
        """Test broadcasting with no connections."""
        message = GraphQLWSMessage(type=MessageType.PING)

        # Should not raise error
        await self.manager.broadcast(message)

    @pytest.mark.asyncio
    async def test_close_all_connections(self):
        """Test closing all connections."""
        mock_ws1 = AsyncMock()
        mock_ws2 = AsyncMock()

        conn1 = await self.manager.add_connection(websocket=mock_ws1)
        conn2 = await self.manager.add_connection(websocket=mock_ws2)

        with (
            patch.object(conn1, "_close") as mock_close1,
            patch.object(conn2, "_close") as mock_close2,
        ):
            await self.manager.close_all()

            # Both connections should be closed
            mock_close1.assert_called_once()
            mock_close2.assert_called_once()

            # Manager should be empty
            assert len(self.manager.connections) == 0

    @pytest.mark.asyncio
    async def test_close_all_no_connections(self):
        """Test closing all connections when none exist."""
        # Should not raise error
        await self.manager.close_all()


class TestEnums:
    """Test enum classes."""

    def test_connection_state_values(self):
        """Test ConnectionState enum values."""
        assert ConnectionState.CONNECTING.value == "connecting"
        assert ConnectionState.READY.value == "ready"
        assert ConnectionState.CLOSING.value == "closing"
        assert ConnectionState.CLOSED.value == "closed"

    def test_subprotocol_values(self):
        """Test SubProtocol enum values."""
        assert SubProtocol.GRAPHQL_WS.value == "graphql-ws"
        assert SubProtocol.GRAPHQL_TRANSPORT_WS.value == "graphql-transport-ws"

    def test_message_type_constants(self):
        """Test MessageType class constants."""
        # Client to server
        assert MessageType.CONNECTION_INIT == "connection_init"
        assert MessageType.SUBSCRIBE == "subscribe"
        assert MessageType.COMPLETE == "complete"
        assert MessageType.PING == "ping"

        # Server to client
        assert MessageType.CONNECTION_ACK == "connection_ack"
        assert MessageType.NEXT == "next"
        assert MessageType.DATA == "data"
        assert MessageType.ERROR == "error"
        assert MessageType.PONG == "pong"

        # Legacy aliases
        assert MessageType.START == "start"
        assert MessageType.STOP == "stop"


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def setup_method(self):
        """Set up test fixtures."""
        self.mock_websocket = AsyncMock()

    @pytest.mark.asyncio
    async def test_connection_handle_full_lifecycle(self):
        """Test full connection handle lifecycle."""
        connection = WebSocketConnection(
            websocket=self.mock_websocket, connection_init_timeout=0.1, keep_alive_interval=0.05
        )

        # Mock connection init then disconnect
        init_msg = GraphQLWSMessage(type=MessageType.CONNECTION_INIT)
        self.mock_websocket.receive.side_effect = [
            {"text": json.dumps(init_msg.to_dict())},  # Init message
            {"type": "websocket.disconnect"},  # Then disconnect
        ]

        # Should complete without error
        await connection.handle()

        assert connection.state == ConnectionState.CLOSED

    @pytest.mark.asyncio
    async def test_connection_handle_cancelled(self):
        """Test connection handle with cancellation."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        # Mock hanging receive to simulate cancellation
        self.mock_websocket.receive.side_effect = asyncio.CancelledError()

        # Should handle cancellation gracefully
        await connection.handle()

    @pytest.mark.asyncio
    async def test_connection_handle_general_exception(self):
        """Test connection handle with general exception."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        # Mock exception during receive
        self.mock_websocket.receive.side_effect = RuntimeError("Unexpected error")

        with patch.object(connection, "_send_error") as mock_send_error:
            await connection.handle()
            mock_send_error.assert_called_once()

    @pytest.mark.asyncio
    async def test_subscription_generator_cancelled(self):
        """Test subscription generator with cancellation."""
        connection = WebSocketConnection(websocket=self.mock_websocket)

        async def cancelled_results():
            await asyncio.sleep(0.1)  # Simulate work
            yield ExecutionResult(data={"test": "value"})

        # Start the generator task and immediately cancel it
        task = asyncio.create_task(
            connection._handle_subscription_generator("sub1", cancelled_results())
        )
        await asyncio.sleep(0.01)  # Let it start
        task.cancel()

        with pytest.raises(asyncio.CancelledError):
            await task

    @pytest.mark.asyncio
    async def test_keep_alive_exception(self):
        """Test keep-alive with send exception."""
        connection = WebSocketConnection(websocket=self.mock_websocket, keep_alive_interval=0.01)
        connection.state = ConnectionState.READY

        # Make send_message fail
        with patch.object(connection, "send_message", side_effect=Exception("Send failed")):
            keep_alive_task = asyncio.create_task(connection._keep_alive())
            await asyncio.sleep(0.05)  # Let it try to send

            # Task should have stopped due to exception
            assert keep_alive_task.done()

    @pytest.mark.asyncio
    async def test_message_disconnect_handling(self):
        """Test handling disconnect during message loop."""
        connection = WebSocketConnection(websocket=self.mock_websocket)
        connection.state = ConnectionState.READY

        # Simulate disconnect message
        self.mock_websocket.receive.return_value = {"type": "websocket.disconnect"}

        # Should break out of loop cleanly
        await connection._message_loop()

    def test_message_creation_edge_cases(self):
        """Test GraphQLWSMessage creation edge cases."""
        # Message with None values
        message = GraphQLWSMessage(type="test", id=None, payload=None)
        result = message.to_dict()
        assert result == {"type": "test"}

        # Message with empty string ID
        message = GraphQLWSMessage(type="test", id="", payload={})
        result = message.to_dict()
        assert result == {"type": "test", "id": "", "payload": {}}
