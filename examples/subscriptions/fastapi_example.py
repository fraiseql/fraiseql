"""
Complete FastAPI application with GraphQL subscriptions.

Features:
- WebSocket endpoint for GraphQL subscriptions
- REST endpoint for publishing events
- HTML client for testing
- Real-time user status updates

Run:
    uvicorn fastapi_example:app --reload

Test:
    Open http://localhost:8000 in browser
    Or connect via WebSocket to ws://localhost:8000/graphql/subscriptions
"""

import asyncio
import json
import uuid
from datetime import datetime
from typing import Optional

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Initialize FastAPI app
app = FastAPI(title="GraphQL Subscriptions Demo")

# Initialize subscription manager with memory event bus
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)


# ============================================================================
# Resolver Functions
# ============================================================================

async def user_subscription_resolver(event: dict, variables: dict) -> dict:
    """Transform user event into subscription response."""
    return {
        "user": {
            "id": str(event.get("id")),
            "name": event.get("name"),
            "status": event.get("status", "unknown"),
            "timestamp": event.get("timestamp"),
        }
    }


async def message_subscription_resolver(event: dict, variables: dict) -> dict:
    """Transform message event into subscription response."""
    return {
        "message": {
            "id": str(event.get("id")),
            "text": event.get("text"),
            "author": event.get("author"),
            "timestamp": event.get("timestamp"),
        }
    }


# ============================================================================
# WebSocket Endpoint
# ============================================================================

@app.websocket("/graphql/subscriptions")
async def websocket_endpoint(websocket: WebSocket):
    """GraphQL subscription WebSocket endpoint."""
    await websocket.accept()

    # Track subscriptions for this connection
    subscriptions: dict[str, str] = {}  # {subscription_id: query_type}
    connection_id = str(uuid.uuid4())

    print(f"Client connected: {connection_id}")

    try:
        while True:
            # Receive message from client
            data = await websocket.receive_text()
            message = json.loads(data)
            message_type = message.get("type")

            # Handle subscription messages
            if message_type == "connection_init":
                # Client initiates connection
                await websocket.send_text(json.dumps({
                    "type": "connection_ack"
                }))
                print(f"Connection initialized: {connection_id}")

            elif message_type == "subscribe":
                # Client subscribes to a query
                subscription_id = message.get("id")
                query = message["payload"]["query"]
                variables = message["payload"].get("variables", {})

                try:
                    # Determine which resolver to use based on query
                    if "message" in query:
                        resolver_fn = message_subscription_resolver
                    else:
                        resolver_fn = user_subscription_resolver

                    # Create subscription in manager
                    await manager.create_subscription(
                        subscription_id=subscription_id,
                        connection_id=connection_id,
                        query=query,
                        variables=variables,
                        resolver_fn=resolver_fn,
                        user_id="demo_user",
                        tenant_id="demo_tenant"
                    )

                    subscriptions[subscription_id] = "active"

                    # Send subscription acknowledgment
                    await websocket.send_text(json.dumps({
                        "type": "next",
                        "id": subscription_id,
                        "payload": {"data": {}}
                    }))

                    print(f"Subscription created: {subscription_id}")

                    # Start polling for events for this subscription
                    asyncio.create_task(
                        poll_events(websocket, subscription_id)
                    )

                except Exception as e:
                    # Send error response
                    await websocket.send_text(json.dumps({
                        "type": "error",
                        "id": subscription_id,
                        "payload": {"message": str(e)}
                    }))
                    print(f"Subscription failed: {str(e)}")

            elif message_type == "complete":
                # Client completes subscription
                subscription_id = message.get("id")
                await manager.complete_subscription(subscription_id)
                subscriptions.pop(subscription_id, None)

                await websocket.send_text(json.dumps({
                    "type": "complete",
                    "id": subscription_id
                }))
                print(f"Subscription completed: {subscription_id}")

    except WebSocketDisconnect:
        print(f"Client disconnected: {connection_id}")

    finally:
        # Cleanup all subscriptions on disconnect
        for subscription_id in list(subscriptions.keys()):
            await manager.complete_subscription(subscription_id)
        print(f"Cleanup complete: {connection_id}")


async def poll_events(websocket: WebSocket, subscription_id: str):
    """Poll for events and send them to client."""
    try:
        while True:
            # Poll for next event (non-blocking)
            response = await manager.get_next_event(subscription_id)

            if response:
                # Send event to client
                await websocket.send_bytes(response)
            else:
                # No event yet, wait a bit before polling again
                await asyncio.sleep(0.01)

    except Exception as e:
        print(f"Error polling events for {subscription_id}: {e}")


# ============================================================================
# REST Endpoints
# ============================================================================

@app.post("/api/users/online")
async def user_came_online(user_id: str, name: str):
    """Publish event when user comes online."""
    event_id = str(uuid.uuid4())

    await manager.publish_event(
        event_type="userOnline",
        channel="users",
        data={
            "id": event_id,
            "user_id": user_id,
            "name": name,
            "status": "online",
            "timestamp": datetime.now().isoformat()
        }
    )

    return {
        "status": "published",
        "event_id": event_id,
        "user": name
    }


@app.post("/api/users/offline")
async def user_went_offline(user_id: str, name: str):
    """Publish event when user goes offline."""
    event_id = str(uuid.uuid4())

    await manager.publish_event(
        event_type="userOffline",
        channel="users",
        data={
            "id": event_id,
            "user_id": user_id,
            "name": name,
            "status": "offline",
            "timestamp": datetime.now().isoformat()
        }
    )

    return {
        "status": "published",
        "event_id": event_id,
        "user": name
    }


@app.post("/api/messages")
async def send_message(author: str, text: str):
    """Publish a message event."""
    message_id = str(uuid.uuid4())

    await manager.publish_event(
        event_type="messagePosted",
        channel="messages",
        data={
            "id": message_id,
            "author": author,
            "text": text,
            "timestamp": datetime.now().isoformat()
        }
    )

    return {
        "status": "published",
        "message_id": message_id,
        "author": author,
        "text": text
    }


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy"}


# ============================================================================
# HTML Client
# ============================================================================

@app.get("/")
async def get_client():
    """Serve HTML client for testing."""
    return HTMLResponse("""
    <!DOCTYPE html>
    <html>
    <head>
        <title>GraphQL Subscriptions Demo</title>
        <style>
            body { font-family: Arial, sans-serif; margin: 20px; }
            .container { max-width: 1200px; margin: 0 auto; }
            .section { margin: 20px 0; padding: 10px; border: 1px solid #ccc; }
            input { padding: 5px; margin: 5px; }
            button { padding: 8px 15px; margin: 5px; cursor: pointer; background: #007bff; color: white; border: none; border-radius: 3px; }
            button:hover { background: #0056b3; }
            .log { background: #f5f5f5; border: 1px solid #ddd; padding: 10px; height: 300px; overflow-y: auto; font-family: monospace; font-size: 12px; }
            .event { margin: 5px 0; padding: 5px; background: white; border-left: 3px solid #007bff; }
            h2 { color: #333; }
        </style>
    </head>
    <body>
        <div class="container">
            <h1>GraphQL Subscriptions Demo</h1>

            <div class="section">
                <h2>User Status Updates</h2>
                <input type="text" id="userName" placeholder="Enter user name" value="Alice">
                <button onclick="userComeOnline()">User Online</button>
                <button onclick="userGoneOffline()">User Offline</button>
            </div>

            <div class="section">
                <h2>Send Messages</h2>
                <input type="text" id="author" placeholder="Author" value="Bob">
                <input type="text" id="messageText" placeholder="Message" value="Hello!">
                <button onclick="sendMessage()">Send Message</button>
            </div>

            <div class="section">
                <h2>Subscriptions</h2>
                <button onclick="subscribeToUsers()">Subscribe to Users</button>
                <button onclick="subscribeToMessages()">Subscribe to Messages</button>
                <button onclick="closeSubscriptions()">Close All</button>
            </div>

            <div class="section">
                <h2>Events Log</h2>
                <div class="log" id="eventLog"></div>
                <button onclick="clearLog()">Clear Log</button>
            </div>
        </div>

        <script>
        let ws = null;
        let subscriptionCount = 0;

        function log(message) {
            const logDiv = document.getElementById('eventLog');
            const time = new Date().toLocaleTimeString();
            const entry = document.createElement('div');
            entry.className = 'event';
            entry.textContent = `[${time}] ${message}`;
            logDiv.appendChild(entry);
            logDiv.scrollTop = logDiv.scrollHeight;
        }

        function clearLog() {
            document.getElementById('eventLog').innerHTML = '';
        }

        function connectWebSocket() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                return;
            }

            ws = new WebSocket('ws://localhost:8000/graphql/subscriptions');

            ws.onopen = () => {
                log('Connected to WebSocket');
                ws.send(JSON.stringify({ type: 'connection_init', payload: {} }));
            };

            ws.onmessage = (event) => {
                const message = JSON.parse(event.data);
                if (message.type === 'connection_ack') {
                    log('Connection acknowledged');
                } else if (message.type === 'next') {
                    const data = message.payload.data;
                    if (data.user) {
                        log(`👤 User: ${data.user.name} is ${data.user.status}`);
                    } else if (data.message) {
                        log(`💬 Message from ${data.message.author}: ${data.message.text}`);
                    }
                } else {
                    log(`Received: ${JSON.stringify(message)}`);
                }
            };

            ws.onerror = (error) => {
                log(`❌ Error: ${error}`);
            };

            ws.onclose = () => {
                log('Disconnected from WebSocket');
            };
        }

        function subscribeToUsers() {
            connectWebSocket();
            subscriptionCount++;
            const subId = `sub_users_${subscriptionCount}`;

            ws.send(JSON.stringify({
                type: 'subscribe',
                id: subId,
                payload: {
                    query: 'subscription { user { id name status timestamp } }'
                }
            }));

            log(`Subscribed to users: ${subId}`);
        }

        function subscribeToMessages() {
            connectWebSocket();
            subscriptionCount++;
            const subId = `sub_messages_${subscriptionCount}`;

            ws.send(JSON.stringify({
                type: 'subscribe',
                id: subId,
                payload: {
                    query: 'subscription { message { id text author timestamp } }'
                }
            }));

            log(`Subscribed to messages: ${subId}`);
        }

        function closeSubscriptions() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.close();
                ws = null;
                log('Closed all subscriptions');
            }
        }

        async function userComeOnline() {
            const name = document.getElementById('userName').value;
            try {
                const response = await fetch(`/api/users/online?user_id=1&name=${encodeURIComponent(name)}`, {
                    method: 'POST'
                });
                const data = await response.json();
                log(`Published: ${name} came online`);
            } catch (e) {
                log(`Error: ${e}`);
            }
        }

        async function userGoneOffline() {
            const name = document.getElementById('userName').value;
            try {
                const response = await fetch(`/api/users/offline?user_id=1&name=${encodeURIComponent(name)}`, {
                    method: 'POST'
                });
                log(`Published: ${name} went offline`);
            } catch (e) {
                log(`Error: ${e}`);
            }
        }

        async function sendMessage() {
            const author = document.getElementById('author').value;
            const text = document.getElementById('messageText').value;
            try {
                const response = await fetch(`/api/messages?author=${encodeURIComponent(author)}&text=${encodeURIComponent(text)}`, {
                    method: 'POST'
                });
                log(`Published message from ${author}: ${text}`);
            } catch (e) {
                log(`Error: ${e}`);
            }
        }

        // Initial log message
        log('Welcome! Click buttons to subscribe or publish events');
        </script>
    </body>
    </html>
    """)


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
