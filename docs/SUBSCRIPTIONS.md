# Subscriptions

FraiseQL provides powerful real-time subscription capabilities using WebSockets and async generators, allowing clients to receive live updates from your GraphQL API.

## Overview

Subscriptions in FraiseQL enable real-time communication between your server and clients. They're perfect for:

- Live notifications
- Real-time data updates
- Chat applications
- Live dashboards
- Collaborative editing

## Basic Usage

### Simple Subscription

```python
from typing import AsyncGenerator
import fraiseql

@fraiseql.subscription
async def message_updates(info) -> AsyncGenerator[str, None]:
    """Stream live messages."""
    async for message in message_stream():
        yield message
```

### Object Subscriptions

Return complex objects:

```python
@fraiseql.type
class Message:
    id: int
    content: str
    user_id: int
    created_at: datetime

@fraiseql.subscription
async def messages(info) -> AsyncGenerator[Message, None]:
    """Stream live message objects."""
    async for message_data in message_stream():
        yield Message(**message_data)
```

### Filtered Subscriptions

Add parameters to filter subscription data:

```python
@fraiseql.subscription
async def user_messages(info, user_id: int) -> AsyncGenerator[Message, None]:
    """Stream messages for a specific user."""
    async for message in message_stream():
        if message.user_id == user_id:
            yield message
```

## Subscription Sources

### Database Change Streams

Listen to database changes:

```python
import asyncpg

@fraiseql.subscription
async def post_updates(info) -> AsyncGenerator['Post', None]:
    """Stream database changes for posts."""

    async with asyncpg.connect(DATABASE_URL) as conn:
        await conn.add_listener('post_changes', message_handler)

        async for notification in listen_for_changes(conn):
            post_data = json.loads(notification.payload)
            yield Post(**post_data)
```

### Message Queues

Use Redis or other message queues:

```python
import redis.asyncio as redis

@fraiseql.subscription
async def notifications(info, user_id: int) -> AsyncGenerator['Notification', None]:
    """Stream notifications from Redis."""

    r = redis.Redis()
    pubsub = r.pubsub()
    await pubsub.subscribe(f'user:{user_id}:notifications')

    async for message in pubsub.listen():
        if message['type'] == 'message':
            notification_data = json.loads(message['data'])
            yield Notification(**notification_data)
```

### WebSocket Connections

Handle direct WebSocket communication:

```python
from fraiseql.subscriptions import WebSocketManager

websocket_manager = WebSocketManager()

@fraiseql.subscription
async def live_updates(info) -> AsyncGenerator[dict, None]:
    """Stream live updates via WebSocket."""

    # Register connection
    connection_id = await websocket_manager.connect(info.context['websocket'])

    try:
        async for update in websocket_manager.listen(connection_id):
            yield update
    finally:
        await websocket_manager.disconnect(connection_id)
```

## Advanced Subscription Features

### Authentication

Secure subscriptions with authentication:

```python
from fraiseql.auth import requires_auth

@fraiseql.subscription
@requires_auth
async def private_messages(info) -> AsyncGenerator[Message, None]:
    """Stream private messages for authenticated users."""
    user = info.context['user']

    async for message in private_message_stream(user.id):
        yield message
```

### Rate Limiting

Implement rate limiting:

```python
from fraiseql.subscriptions import rate_limit

@fraiseql.subscription
@rate_limit(max_events=10, window=60)  # 10 events per minute
async def throttled_updates(info) -> AsyncGenerator[str, None]:
    """Rate-limited subscription."""
    async for update in update_stream():
        yield update
```

### Subscription Complexity

Control subscription complexity:

```python
from fraiseql.subscriptions import complexity

@fraiseql.subscription
@complexity(score=5, max_depth=3)
async def complex_subscription(info) -> AsyncGenerator['ComplexType', None]:
    """Subscription with complexity limits."""
    async for data in complex_data_stream():
        yield data
```

### Lifecycle Hooks

Add lifecycle management:

```python
from fraiseql.subscriptions import with_lifecycle

async def on_subscribe(info):
    print(f"User {info.context['user'].id} subscribed")

async def on_complete(info):
    print(f"Subscription completed for user {info.context['user'].id}")

@fraiseql.subscription
@with_lifecycle(on_start=on_subscribe, on_complete=on_complete)
async def tracked_updates(info) -> AsyncGenerator[str, None]:
    """Subscription with lifecycle tracking."""
    async for update in update_stream():
        yield update
```

## Client Integration

### JavaScript/React

```javascript
import { createClient } from 'graphql-ws';
import { useSubscription } from 'urql';

const wsClient = createClient({
  url: 'ws://localhost:8000/graphql/ws',
});

// React component
function LiveMessages() {
  const [result] = useSubscription({
    query: `
      subscription {
        messages {
          id
          content
          createdAt
        }
      }
    `,
  });

  if (result.data) {
    return <div>{result.data.messages.content}</div>;
  }

  return <div>Loading...</div>;
}
```

### Python Client

```python
import asyncio
import websockets
from gql import Client
from gql.transport.websockets import WebsocketsTransport

async def subscription_client():
    transport = WebsocketsTransport(url="ws://localhost:8000/graphql/ws")

    async with Client(transport=transport) as session:
        subscription = gql("""
            subscription {
                messages {
                    id
                    content
                    createdAt
                }
            }
        """)

        async for result in session.subscribe(subscription):
            print(f"Received: {result}")
```

## Error Handling

### Graceful Error Handling

Handle errors in subscriptions:

```python
import logging

logger = logging.getLogger(__name__)

@fraiseql.subscription
async def robust_subscription(info) -> AsyncGenerator[Message, None]:
    """Subscription with error handling."""
    try:
        async for message in message_stream():
            yield message
    except ConnectionError as e:
        logger.error(f"Connection error in subscription: {e}")
        # Could yield error message or reconnect
    except Exception as e:
        logger.error(f"Unexpected error in subscription: {e}")
        raise  # Re-raise for GraphQL error handling
```

### Error Recovery

Implement automatic recovery:

```python
@fraiseql.subscription
async def resilient_subscription(info) -> AsyncGenerator[Message, None]:
    """Auto-recovering subscription."""
    retry_count = 0
    max_retries = 3

    while retry_count < max_retries:
        try:
            async for message in message_stream():
                yield message
                retry_count = 0  # Reset on successful message
        except Exception as e:
            retry_count += 1
            if retry_count >= max_retries:
                raise

            await asyncio.sleep(2 ** retry_count)  # Exponential backoff
```

## Performance Optimization

### Connection Pooling

Manage subscription connections efficiently:

```python
from fraiseql.subscriptions import ConnectionPool

connection_pool = ConnectionPool(max_connections=1000)

@fraiseql.subscription
async def pooled_subscription(info) -> AsyncGenerator[Message, None]:
    """Subscription using connection pool."""
    async with connection_pool.get_connection() as conn:
        async for message in conn.listen():
            yield message
```

### Memory Management

Handle memory efficiently in long-running subscriptions:

```python
@fraiseql.subscription
async def memory_efficient_subscription(info) -> AsyncGenerator[Message, None]:
    """Memory-efficient subscription."""
    buffer_size = 100
    buffer = []

    async for message in message_stream():
        buffer.append(message)

        if len(buffer) >= buffer_size:
            # Yield buffered messages
            for msg in buffer:
                yield msg
            buffer.clear()
```

### Batching Updates

Batch multiple updates:

```python
@fraiseql.subscription
async def batched_updates(info) -> AsyncGenerator[list[Message], None]:
    """Batch updates for efficiency."""
    batch_size = 10
    batch_timeout = 1.0  # seconds

    batch = []
    last_yield = time.time()

    async for message in message_stream():
        batch.append(message)

        if len(batch) >= batch_size or (time.time() - last_yield) >= batch_timeout:
            yield batch
            batch = []
            last_yield = time.time()
```

## Testing Subscriptions

### Unit Testing

Test subscription logic:

```python
import pytest
from unittest.mock import AsyncMock

@pytest.mark.asyncio
async def test_message_subscription():
    """Test message subscription logic."""

    # Mock the message stream
    async def mock_stream():
        yield {"id": 1, "content": "Hello"}
        yield {"id": 2, "content": "World"}

    # Test subscription
    messages = []
    async for message in message_subscription_handler(mock_stream()):
        messages.append(message)

    assert len(messages) == 2
    assert messages[0].content == "Hello"
```

### Integration Testing

Test with real WebSocket connections:

```python
import pytest
import websockets
from fraiseql.testing import GraphQLWebSocketClient

@pytest.mark.asyncio
async def test_subscription_integration():
    """Test subscription with WebSocket client."""

    client = GraphQLWebSocketClient("ws://localhost:8000/graphql/ws")

    subscription = """
        subscription {
            messages {
                content
            }
        }
    """

    # Subscribe and collect messages
    messages = []
    async for result in client.subscribe(subscription):
        messages.append(result['data']['messages'])
        if len(messages) >= 2:
            break

    assert len(messages) == 2
```

## Best Practices

1. **Handle errors gracefully**: Always implement proper error handling
2. **Use authentication**: Secure sensitive subscriptions
3. **Implement rate limiting**: Prevent abuse and resource exhaustion
4. **Monitor connections**: Track active subscriptions and performance
5. **Clean up resources**: Properly close connections and free memory
6. **Test thoroughly**: Include both unit and integration tests
7. **Use appropriate protocols**: Choose WebSockets vs Server-Sent Events based on needs
8. **Buffer appropriately**: Balance real-time vs resource usage

## Common Patterns

### Live Chat

```python
@fraiseql.subscription
async def chat_messages(info, room_id: str) -> AsyncGenerator[ChatMessage, None]:
    """Live chat messages for a room."""
    async for message in chat_stream(room_id):
        yield message

@fraiseql.mutation
async def send_message(info, room_id: str, content: str) -> ChatMessage:
    """Send a chat message."""
    message = await ChatMessage.create(
        room_id=room_id,
        content=content,
        user_id=info.context['user'].id
    )

    # Notify subscribers
    await publish_to_chat_stream(room_id, message)
    return message
```

### Live Notifications

```python
@fraiseql.subscription
async def user_notifications(info) -> AsyncGenerator[Notification, None]:
    """User-specific notifications."""
    user_id = info.context['user'].id

    async for notification in notification_stream(user_id):
        yield notification

@fraiseql.mutation
async def mark_notification_read(info, notification_id: int) -> Notification:
    """Mark notification as read."""
    notification = await Notification.mark_read(notification_id)

    # Update live subscribers
    await publish_notification_update(notification)
    return notification
```

### Live Dashboards

```python
@fraiseql.subscription
async def dashboard_metrics(info) -> AsyncGenerator[DashboardData, None]:
    """Live dashboard metrics."""
    async for metrics in metrics_stream():
        yield DashboardData(
            active_users=metrics['active_users'],
            revenue=metrics['revenue'],
            orders=metrics['orders']
        )
```

## Deployment Considerations

### Load Balancing

Handle subscriptions with multiple servers:

```python
# Use Redis for cross-server communication
import redis.asyncio as redis

redis_client = redis.Redis()

@fraiseql.subscription
async def distributed_updates(info) -> AsyncGenerator[Message, None]:
    """Subscription that works across multiple servers."""

    # Subscribe to Redis channel
    pubsub = redis_client.pubsub()
    await pubsub.subscribe('global_updates')

    async for message in pubsub.listen():
        if message['type'] == 'message':
            yield Message(**json.loads(message['data']))
```

### Monitoring

Monitor subscription health:

```python
from fraiseql.subscriptions import SubscriptionMetrics

metrics = SubscriptionMetrics()

@fraiseql.subscription
async def monitored_subscription(info) -> AsyncGenerator[Message, None]:
    """Subscription with monitoring."""

    metrics.active_subscriptions.inc()

    try:
        async for message in message_stream():
            metrics.messages_sent.inc()
            yield message
    finally:
        metrics.active_subscriptions.dec()
```
