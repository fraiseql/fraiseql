# Real-time Chat API Example

🟠 ADVANCED | ⏱️ 45 min | 🎯 Real-Time | 🏷️ WebSockets

A comprehensive real-time chat API built with FraiseQL, demonstrating WebSocket subscriptions, PostgreSQL LISTEN/NOTIFY, and advanced real-time features.

**What you'll learn:**
- WebSocket-based real-time messaging
- PostgreSQL LISTEN/NOTIFY for events
- Presence tracking and typing indicators
- Event-driven architecture patterns
- Scalable real-time application design

**Prerequisites:**
- `../enterprise_patterns/` - Enterprise patterns foundation
- Understanding of WebSocket concepts
- Experience with real-time applications

**Next steps:**
- `../analytics_dashboard/` - Add business intelligence
- `../admin-panel/` - Administrative interfaces
- Custom real-time features for your domain

## Features

- **Real-time Messaging**: WebSocket-based instant messaging
- **User Presence**: Online/offline status tracking
- **Typing Indicators**: Show when users are typing
- **Message Reactions**: Emoji reactions on messages
- **Message Threading**: Reply to specific messages
- **Direct Messages**: 1-on-1 private conversations
- **Room Management**: Public and private chat rooms
- **File Attachments**: Images, documents, and media
- **Message Search**: Full-text search across messages
- **Read Receipts**: Track message read status
- **Moderation Tools**: Message deletion, user banning
- **Push Notifications**: Browser push notification support

## Architecture

This example demonstrates FraiseQL's real-time capabilities:

- **PostgreSQL LISTEN/NOTIFY**: Database-driven real-time events
- **WebSocket Management**: Scalable connection handling
- **Event-Driven Architecture**: Reactive real-time updates
- **CQRS Pattern**: Optimized reads and writes
- **Presence Tracking**: Efficient online status management

## Setup

### 1. Database Setup

```bash
# Create database
createdb realtime_chat

# Run migrations
psql -d realtime_chat -f db/migrations/001_chat_schema.sql

# Create views
psql -d realtime_chat -f db/views/chat_views.sql

# Create functions
psql -d realtime_chat -f db/functions/chat_functions.sql

# Load sample data (optional)
psql -d realtime_chat -f db/seeds/sample_data.sql
```

### 2. Environment Setup

```bash
# Create virtual environment
python -m venv venv
source venv/bin/activate

# Install dependencies
pip install fraiseql fastapi uvicorn asyncpg websockets

# Set environment variables
export DATABASE_URL="postgresql://user:password@localhost:5432/realtime_chat"
```

### 3. Run the Application

```bash
# Run the server
uvicorn app:app --reload

# Access the API
# GraphQL Playground: http://localhost:8000/graphql
# WebSocket: ws://localhost:8000/ws/{user_id}
```

## WebSocket Usage

### Connect to WebSocket

```javascript
// Connect to WebSocket
const ws = new WebSocket('ws://localhost:8000/ws/user123');

ws.onopen = function(event) {
    console.log('Connected to chat');

    // Join a room
    ws.send(JSON.stringify({
        type: 'join_room',
        room_id: 'room456'
    }));
};

ws.onmessage = function(event) {
    const data = JSON.parse(event.data);
    console.log('Received:', data);

    switch(data.type) {
        case 'message_event':
            handleNewMessage(data.message);
            break;
        case 'typing_event':
            handleTypingIndicator(data);
            break;
        case 'presence_event':
            handlePresenceUpdate(data);
            break;
    }
};
```

### Typing Indicators

```javascript
// Start typing
ws.send(JSON.stringify({
    type: 'typing_start',
    room_id: 'room456'
}));

// Stop typing
ws.send(JSON.stringify({
    type: 'typing_stop',
    room_id: 'room456'
}));
```

## GraphQL Examples

### Create a Room

```graphql
mutation CreateRoom {
  createRoom(
    name: "General Discussion"
    slug: "general"
    ownerId: "user123"
    type: "public"
    description: "General chat for everyone"
  ) {
    success
    roomId
    message
  }
}
```

### Send a Message

```graphql
mutation SendMessage {
  sendMessage(
    roomId: "room456"
    userId: "user123"
    content: "Hello everyone! 👋"
    messageType: "text"
  ) {
    success
    messageId
    message
  }
}
```

### Get Room Messages

```graphql
query GetMessages($roomId: UUID!, $limit: Int!) {
  messageThread(
    where: { roomId: { _eq: $roomId } }
    orderBy: { createdAt: DESC }
    limit: $limit
  ) {
    id
    content
    messageType
    createdAt
    editedAt
    author {
      id
      username
      displayName
      avatarUrl
    }
    attachments {
      id
      filename
      url
      thumbnailUrl
      mimeType
    }
    reactions {
      emoji
      count
      users {
        username
      }
    }
    replyCount
  }
}
```

### Add Message Reaction

```graphql
mutation AddReaction {
  addMessageReaction(
    messageId: "msg789"
    userId: "user123"
    emoji: "👍"
  ) {
    success
    message
  }
}
```

### Update Presence

```graphql
mutation UpdatePresence {
  updateUserPresence(
    userId: "user123"
    status: "online"
    roomId: "room456"
  ) {
    success
    message
  }
}
```

### Get User Conversations

```graphql
query GetConversations($userId: UUID!) {
  userConversations(
    where: { userId: { _eq: $userId } }
    orderBy: { latestMessage: { createdAt: DESC } }
  ) {
    roomId
    name
    type
    unreadCount
    latestMessage {
      content
      createdAt
      author {
        username
      }
    }
    directUser {
      id
      username
      displayName
      status
    }
  }
}
```

### Create Direct Conversation

```graphql
mutation CreateDM {
  createDirectConversation(
    user1Id: "user123"
    user2Id: "user456"
  ) {
    success
    roomId
    conversationId
    message
  }
}
```

## Real-time Events

### Message Events

Triggered when messages are created, updated, or deleted:

```json
{
  "type": "message_event",
  "event": "INSERT",
  "room_id": "room456",
  "message": {
    "id": "msg789",
    "content": "Hello!",
    "author": {
      "username": "john_doe",
      "displayName": "John Doe"
    },
    "createdAt": "2023-12-01T12:00:00Z"
  }
}
```

### Typing Events

Triggered when users start/stop typing:

```json
{
  "type": "typing_event",
  "event": "INSERT",
  "room_id": "room456",
  "user_id": "user123"
}
```

### Presence Events

Triggered when user status changes:

```json
{
  "type": "presence_event",
  "event": "UPDATE",
  "user_id": "user123",
  "status": "online"
}
```

## Performance Features

### 1. Efficient Presence Tracking

- Automatic cleanup of stale presence records
- Session-based presence management
- Room-specific presence tracking

### 2. Optimized Message Loading

- Cursor-based pagination
- Lazy loading of message threads
- Efficient read receipt tracking

### 3. Smart Notifications

- PostgreSQL LISTEN/NOTIFY for real-time events
- Connection pooling for WebSocket management
- Automatic cleanup of expired typing indicators

### 4. Search Optimization

- Full-text search with PostgreSQL
- Trigram indexes for fuzzy matching
- Search result ranking

## Testing

### WebSocket Testing

```python
import asyncio
import websockets
import json

async def test_websocket():
    uri = "ws://localhost:8000/ws/test_user"

    async with websockets.connect(uri) as websocket:
        # Join a room
        await websocket.send(json.dumps({
            "type": "join_room",
            "room_id": "test_room"
        }))

        # Listen for messages
        async for message in websocket:
            data = json.loads(message)
            print(f"Received: {data}")

asyncio.run(test_websocket())
```

### Load Testing

```bash
# Install artillery
npm install -g artillery

# Run WebSocket load test
artillery run websocket-load-test.yml
```

## Production Considerations

### 1. Scaling WebSockets

- Use Redis for session management across instances
- Implement sticky sessions or session affinity
- Consider using Socket.IO for better browser compatibility

### 2. Message Storage

- Implement message archiving for old conversations
- Use partitioning for large message tables
- Consider message compression for storage efficiency

### 3. Security

- Implement proper JWT authentication
- Rate limiting for message sending
- Content moderation and spam detection
- End-to-end encryption for sensitive conversations

### 4. Monitoring

- Track WebSocket connection metrics
- Monitor message delivery latency
- Alert on failed real-time event delivery
- Track user engagement metrics

## Advanced Features

### 1. Message Threading

```graphql
mutation ReplyToMessage {
  sendMessage(
    roomId: "room456"
    userId: "user123"
    content: "This is a reply"
    parentMessageId: "original_msg_id"
  ) {
    success
    messageId
  }
}
```

### 2. File Uploads

```graphql
mutation SendFileMessage {
  sendMessage(
    roomId: "room456"
    userId: "user123"
    content: "Shared a file"
    messageType: "file"
    metadata: {
      filename: "document.pdf"
      fileSize: 1024000
      mimeType: "application/pdf"
    }
  ) {
    success
    messageId
  }
}
```

### 3. Push Notifications

```graphql
mutation SubscribeToPush {
  subscribeToPushNotifications(
    userId: "user123"
    subscription: {
      endpoint: "https://fcm.googleapis.com/..."
      keys: {
        p256dh: "key..."
        auth: "auth..."
      }
    }
  ) {
    success
  }
}
```

## Next Steps

1. Implement end-to-end encryption
2. Add voice/video calling features
3. Build mobile app with push notifications
4. Add AI-powered chat moderation
5. Implement message translation
6. Add screen sharing capabilities

## Resources

- [WebSocket API Documentation](https://developer.mozilla.org/en-US/docs/Web/API/WebSocket)
- [PostgreSQL LISTEN/NOTIFY](https://www.postgresql.org/docs/current/sql-notify.html)
- [FastAPI WebSockets](https://fastapi.tiangolo.com/advanced/websockets/)
- [Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API)
