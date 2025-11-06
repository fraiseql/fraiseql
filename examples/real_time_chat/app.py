"""Real-time Chat API Application

Demonstrates FraiseQL's real-time capabilities with WebSocket subscriptions
"""

import asyncio
import json
import os
from contextlib import asynccontextmanager, suppress

import asyncpg
from fastapi import Depends, FastAPI, HTTPException, WebSocket, WebSocketDisconnect
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

from fraiseql import FraiseQL

from .models import ChatQuery
from .mutations import (
    add_message_reaction,
    create_direct_conversation,
    create_room,
    delete_message,
    edit_message,
    join_room,
    mark_messages_read,
    remove_message_reaction,
    send_message,
    set_typing_indicator,
    update_user_presence,
)

# Database configuration
DATABASE_URL = os.getenv(
    "DATABASE_URL",
    "postgresql://user:password@localhost:5432/realtime_chat",
)

# Security
security = HTTPBearer()


# Connection manager for WebSocket connections
class ConnectionManager:
    def __init__(self):
        # room_id -> set of websockets
        self.room_connections: dict[str, set[WebSocket]] = {}
        # user_id -> set of websockets
        self.user_connections: dict[str, set[WebSocket]] = {}
        # websocket -> user_id
        self.connection_users: dict[WebSocket, str] = {}

    async def connect(self, websocket: WebSocket, user_id: str, room_id: str | None = None):
        """Connect a WebSocket and associate with user/room"""
        await websocket.accept()

        # Track user connection
        if user_id not in self.user_connections:
            self.user_connections[user_id] = set()
        self.user_connections[user_id].add(websocket)
        self.connection_users[websocket] = user_id

        # Track room connection if specified
        if room_id:
            if room_id not in self.room_connections:
                self.room_connections[room_id] = set()
            self.room_connections[room_id].add(websocket)

    def disconnect(self, websocket: WebSocket):
        """Disconnect a WebSocket"""
        user_id = self.connection_users.pop(websocket, None)

        if user_id and user_id in self.user_connections:
            self.user_connections[user_id].discard(websocket)
            if not self.user_connections[user_id]:
                del self.user_connections[user_id]

        # Remove from room connections
        for connections in self.room_connections.values():
            connections.discard(websocket)

    async def send_to_room(self, room_id: str, message: dict):
        """Send message to all connections in a room"""
        if room_id in self.room_connections:
            disconnected = set()
            for connection in self.room_connections[room_id]:
                try:
                    await connection.send_text(json.dumps(message))
                except Exception:
                    disconnected.add(connection)

            # Clean up disconnected connections
            for connection in disconnected:
                self.disconnect(connection)

    async def send_to_user(self, user_id: str, message: dict):
        """Send message to all connections for a user"""
        if user_id in self.user_connections:
            disconnected = set()
            for connection in self.user_connections[user_id]:
                try:
                    await connection.send_text(json.dumps(message))
                except Exception:
                    disconnected.add(connection)

            # Clean up disconnected connections
            for connection in disconnected:
                self.disconnect(connection)


# Global connection manager
manager = ConnectionManager()


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager"""
    # Create connection pool
    app.state.db_pool = await asyncpg.create_pool(
        DATABASE_URL,
        min_size=10,
        max_size=20,
        command_timeout=60,
    )

    # Start PostgreSQL LISTEN task
    listen_task = asyncio.create_task(listen_for_notifications(app.state.db_pool))

    yield

    # Cancel the listen task and close connection pool
    listen_task.cancel()
    with suppress(asyncio.CancelledError):
        await listen_task
    await app.state.db_pool.close()


async def listen_for_notifications(pool):
    """Listen for PostgreSQL NOTIFY events and broadcast via WebSocket"""
    async with pool.acquire() as conn:
        await conn.add_listener("message_event", handle_message_event)
        await conn.add_listener("typing_event", handle_typing_event)
        await conn.add_listener("presence_event", handle_presence_event)

        # Keep the connection alive
        try:
            while True:
                await asyncio.sleep(1)
        except asyncio.CancelledError:
            pass


async def handle_message_event(connection, pid, channel, payload):
    """Handle message events from PostgreSQL"""
    try:
        data = json.loads(payload)
        room_id = data["room_id"]

        # Fetch the full message data
        message_data = await fetch_message_data(connection, data["message_id"])

        event = {
            "type": "message_event",
            "event": data["event"],
            "room_id": room_id,
            "message": message_data,
        }

        await manager.send_to_room(str(room_id), event)
    except Exception:
        pass


async def handle_typing_event(connection, pid, channel, payload):
    """Handle typing events from PostgreSQL"""
    try:
        data = json.loads(payload)
        room_id = data["room_id"]

        event = {
            "type": "typing_event",
            "event": data["event"],
            "room_id": room_id,
            "user_id": data["user_id"],
        }

        await manager.send_to_room(str(room_id), event)
    except Exception:
        pass


async def handle_presence_event(connection, pid, channel, payload):
    """Handle presence events from PostgreSQL"""
    try:
        data = json.loads(payload)
        user_id = data["user_id"]

        event = {
            "type": "presence_event",
            "event": data["event"],
            "user_id": user_id,
            "status": data["status"],
        }

        # Send to user's connections and relevant room connections
        await manager.send_to_user(str(user_id), event)

        if data.get("room_id"):
            await manager.send_to_room(str(data["room_id"]), event)
    except Exception:
        pass


async def fetch_message_data(connection, message_id):
    """Fetch full message data for real-time updates"""
    query = """
    SELECT row_to_json(mt.*) as message
    FROM v_message_thread mt
    WHERE mt.id = $1
    """
    row = await connection.fetchrow(query, message_id)
    return row["message"] if row else None


# Authentication helper
async def get_current_user(
    credentials: HTTPAuthorizationCredentials = Depends(security),
):
    """Extract user from JWT token (simplified for demo)"""
    # In production, verify JWT token here
    token = credentials.credentials
    # For demo, assume token is just the user_id
    try:
        return token  # This should be proper JWT validation
    except Exception as e:
        raise HTTPException(status_code=401, detail="Invalid token") from e


# Create FastAPI app
app = FastAPI(
    title="Real-time Chat API",
    description="""
    A real-time chat API built with FraiseQL and WebSocket subscriptions.

    Features:
    - Real-time messaging with WebSocket subscriptions
    - User presence tracking
    - Typing indicators
    - Message reactions and threading
    - Direct messaging
    - Room-based chat with permissions
    - Message read receipts
    - File attachments
    - Search functionality
    - Moderation tools

    This example demonstrates:
    - PostgreSQL LISTEN/NOTIFY for real-time events
    - WebSocket connection management
    - GraphQL subscriptions
    - Real-time presence tracking
    - Scalable chat architecture
    """,
    version="1.0.0",
    lifespan=lifespan,
)


# Create FraiseQL instance
fraiseql = FraiseQL(
    db_url=DATABASE_URL,
    query_type=ChatQuery,
    mutations=[
        create_room,
        join_room,
        send_message,
        edit_message,
        delete_message,
        add_message_reaction,
        remove_message_reaction,
        update_user_presence,
        set_typing_indicator,
        mark_messages_read,
        create_direct_conversation,
    ],
)


# Add GraphQL endpoint
fraiseql.attach_to_app(app, path="/graphql")


# WebSocket endpoint for real-time features
@app.websocket("/ws/{user_id}")
async def websocket_endpoint(websocket: WebSocket, user_id: str):
    """WebSocket endpoint for real-time chat features"""
    await manager.connect(websocket, user_id)

    try:
        # Update user presence to online
        async with app.state.db_pool.acquire() as conn:
            await conn.execute(
                "SELECT update_user_presence($1, 'online', NULL, $2)",
                user_id,
                websocket.client.host,
            )

        while True:
            # Receive messages from client
            data = await websocket.receive_text()
            message = json.loads(data)

            # Handle different message types
            if message["type"] == "join_room":
                room_id = message["room_id"]
                if room_id not in manager.room_connections:
                    manager.room_connections[room_id] = set()
                manager.room_connections[room_id].add(websocket)

            elif message["type"] == "leave_room":
                room_id = message["room_id"]
                if room_id in manager.room_connections:
                    manager.room_connections[room_id].discard(websocket)

            elif message["type"] == "typing_start":
                room_id = message["room_id"]
                async with app.state.db_pool.acquire() as conn:
                    await conn.execute(
                        "SELECT set_typing_indicator($1, $2, true)",
                        room_id,
                        user_id,
                    )

            elif message["type"] == "typing_stop":
                room_id = message["room_id"]
                async with app.state.db_pool.acquire() as conn:
                    await conn.execute(
                        "SELECT set_typing_indicator($1, $2, false)",
                        room_id,
                        user_id,
                    )

    except WebSocketDisconnect:
        manager.disconnect(websocket)

        # Update user presence to offline
        async with app.state.db_pool.acquire() as conn:
            await conn.execute("SELECT update_user_presence($1, 'offline')", user_id)


# REST endpoints
@app.get("/")
async def root():
    """API root endpoint"""
    return {
        "message": "Real-time Chat API",
        "graphql": "/graphql",
        "websocket": "/ws/{user_id}",
        "docs": "/docs",
        "health": "/health",
    }


@app.get("/health")
async def health_check():
    """Health check endpoint"""
    try:
        async with app.state.db_pool.acquire() as conn:
            await conn.fetchval("SELECT 1")

        return {
            "status": "healthy",
            "database": "connected",
            "active_connections": len(manager.connection_users),
        }
    except Exception as e:
        return {"status": "unhealthy", "database": "disconnected", "error": str(e)}


@app.get("/api/rooms/{room_id}/messages")
async def get_room_messages(
    room_id: str,
    limit: int = 50,
    offset: int = 0,
    before: str | None = None,
    user_id: str = Depends(get_current_user),
):
    """REST endpoint for fetching room messages with pagination"""
    query = """
    query GetRoomMessages($roomId: UUID!, $limit: Int!, $offset: Int!, $before: DateTime) {
        messageThread(
            where: {
                roomId: {_eq: $roomId},
                createdAt: {_lt: $before}
            },
            orderBy: {createdAt: DESC},
            limit: $limit,
            offset: $offset
        ) {
            id
            content
            messageType
            createdAt
            editedAt
            author
            attachments
            reactions
            replyCount
        }
    }
    """

    variables = {"roomId": room_id, "limit": limit, "offset": offset, "before": before}

    result = await fraiseql.execute(query, variables)
    return result.get("data", {}).get("messageThread", [])


@app.get("/api/users/{user_id}/conversations")
async def get_user_conversations(
    user_id: str,
    current_user: Annotated[str, Depends(get_current_user)],
):
    """REST endpoint for fetching user's conversations"""
    if user_id != current_user:
        raise HTTPException(status_code=403, detail="Access denied")

    query = """
    query GetUserConversations($userId: UUID!) {
        userConversations(
            where: {userId: {_eq: $userId}},
            orderBy: {latestMessage: {createdAt: DESC}}
        ) {
            roomId
            name
            type
            role
            unreadCount
            latestMessage
            directUser
        }
    }
    """

    variables = {"userId": user_id}
    result = await fraiseql.execute(query, variables)
    return result.get("data", {}).get("userConversations", [])


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
