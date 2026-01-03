"""
Real-World Example: Multi-User Chat Application

Features:
- User presence tracking (online/offline/typing)
- Real-time message delivery
- User list with online status
- Typing indicators
- Message history
- Persistent user sessions

Run:
    uvicorn realworld_chat:app --reload

Open multiple browser tabs to test multi-user scenarios.
"""

import json
import uuid
from datetime import datetime
from typing import Optional, Set
from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

app = FastAPI(title="Multi-User Chat")

# Initialize subscription manager
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)

# In-memory data storage
users: dict[str, dict] = {}  # {user_id: {name, status, session_id}}
messages: list[dict] = []  # Chat history
max_history = 50  # Keep last 50 messages


# ============================================================================
# Resolvers
# ============================================================================

async def user_presence_resolver(event: dict, variables: dict) -> dict:
    """Transform user presence event into subscription response."""
    return {
        "userPresence": {
            "userId": event.get("user_id"),
            "name": event.get("name"),
            "status": event.get("status"),  # online, offline, typing
            "timestamp": event.get("timestamp")
        }
    }


async def message_resolver(event: dict, variables: dict) -> dict:
    """Transform message event into subscription response."""
    return {
        "message": {
            "id": event.get("id"),
            "author": event.get("author"),
            "text": event.get("text"),
            "timestamp": event.get("timestamp")
        }
    }


async def user_list_resolver(event: dict, variables: dict) -> dict:
    """Return current user list."""
    user_list = [
        {
            "id": uid,
            "name": udata.get("name"),
            "status": udata.get("status", "offline")
        }
        for uid, udata in users.items()
    ]
    return {"userList": user_list}


# ============================================================================
# WebSocket Handler
# ============================================================================

active_connections: dict[str, WebSocket] = {}


@app.websocket("/ws/{user_id}/{name}")
async def websocket_endpoint(websocket: WebSocket, user_id: str, name: str):
    """Chat WebSocket endpoint."""
    await websocket.accept()

    session_id = str(uuid.uuid4())
    subscriptions: dict[str, str] = {}

    # Register user
    users[user_id] = {
        "name": name,
        "status": "online",
        "session_id": session_id
    }
    active_connections[session_id] = websocket

    print(f"User {name} ({user_id}) connected")

    # Publish user online event
    await manager.publish_event(
        event_type="userOnline",
        channel="presence",
        data={
            "user_id": user_id,
            "name": name,
            "status": "online",
            "timestamp": datetime.now().isoformat()
        }
    )

    try:
        while True:
            data = await websocket.receive_text()
            message = json.loads(data)
            message_type = message.get("type")

            if message_type == "subscribe":
                # Client subscribing
                sub_id = message.get("id")
                query = message["payload"]["query"]

                # Determine resolver
                if "userPresence" in query:
                    resolver = user_presence_resolver
                    channel = "presence"
                elif "userList" in query:
                    resolver = user_list_resolver
                    channel = "userlist"
                else:
                    resolver = message_resolver
                    channel = "messages"

                # Create subscription
                await manager.create_subscription(
                    subscription_id=sub_id,
                    connection_id=session_id,
                    query=query,
                    variables=message["payload"].get("variables", {}),
                    resolver_fn=resolver,
                    user_id=user_id,
                    tenant_id="chat_app"
                )

                subscriptions[sub_id] = channel

                # Start polling
                import asyncio
                asyncio.create_task(
                    poll_events(websocket, sub_id)
                )

            elif message_type == "message":
                # User sending a chat message
                msg_text = message.get("text")

                msg = {
                    "id": str(uuid.uuid4()),
                    "author": name,
                    "author_id": user_id,
                    "text": msg_text,
                    "timestamp": datetime.now().isoformat()
                }

                # Store in history
                messages.append(msg)
                if len(messages) > max_history:
                    messages.pop(0)

                # Publish to subscribers
                await manager.publish_event(
                    event_type="messagePosted",
                    channel="messages",
                    data=msg
                )

                print(f"{name}: {msg_text}")

            elif message_type == "typing":
                # User typing indicator
                typing = message.get("typing", False)
                status = "typing" if typing else "online"

                users[user_id]["status"] = status

                await manager.publish_event(
                    event_type="userTyping",
                    channel="presence",
                    data={
                        "user_id": user_id,
                        "name": name,
                        "status": status,
                        "timestamp": datetime.now().isoformat()
                    }
                )

    except WebSocketDisconnect:
        print(f"User {name} ({user_id}) disconnected")

    finally:
        # Cleanup
        del users[user_id]
        del active_connections[session_id]

        # Publish user offline event
        await manager.publish_event(
            event_type="userOffline",
            channel="presence",
            data={
                "user_id": user_id,
                "name": name,
                "status": "offline",
                "timestamp": datetime.now().isoformat()
            }
        )

        # Complete all subscriptions
        for sub_id in subscriptions:
            await manager.complete_subscription(sub_id)


async def poll_events(websocket: WebSocket, sub_id: str):
    """Poll for events."""
    import asyncio
    try:
        while True:
            response = await manager.get_next_event(sub_id)
            if response:
                await websocket.send_bytes(response)
            else:
                await asyncio.sleep(0.01)
    except Exception as e:
        print(f"Error: {e}")


# ============================================================================
# REST Endpoints
# ============================================================================

@app.get("/api/users")
async def get_users():
    """Get list of online users."""
    return {
        "users": [
            {
                "id": uid,
                "name": udata["name"],
                "status": udata["status"]
            }
            for uid, udata in users.items()
        ]
    }


@app.get("/api/messages")
async def get_messages(limit: int = 50):
    """Get chat history."""
    return {
        "messages": messages[-limit:]
    }


# ============================================================================
# HTML Client
# ============================================================================

@app.get("/")
async def get_client():
    """Serve HTML chat client."""
    return HTMLResponse("""
    <!DOCTYPE html>
    <html>
    <head>
        <title>Chat Room</title>
        <style>
            body {
                font-family: Arial, sans-serif;
                max-width: 1000px;
                margin: 0 auto;
                padding: 20px;
                background: #f5f5f5;
            }
            .container {
                display: flex;
                gap: 20px;
                height: 600px;
            }
            .sidebar {
                width: 200px;
                background: white;
                border-radius: 5px;
                padding: 15px;
                box-shadow: 0 2px 5px rgba(0,0,0,0.1);
            }
            .chat {
                flex: 1;
                display: flex;
                flex-direction: column;
                background: white;
                border-radius: 5px;
                box-shadow: 0 2px 5px rgba(0,0,0,0.1);
                overflow: hidden;
            }
            .messages {
                flex: 1;
                overflow-y: auto;
                padding: 15px;
                border-bottom: 1px solid #ddd;
            }
            .message {
                margin: 10px 0;
                padding: 10px;
                background: #f9f9f9;
                border-left: 3px solid #007bff;
                border-radius: 3px;
            }
            .message .author {
                font-weight: bold;
                color: #007bff;
            }
            .message .time {
                font-size: 0.8em;
                color: #999;
            }
            .input-area {
                padding: 15px;
                display: flex;
                gap: 10px;
            }
            #messageInput {
                flex: 1;
                padding: 10px;
                border: 1px solid #ddd;
                border-radius: 3px;
                font-size: 14px;
            }
            button {
                padding: 10px 20px;
                background: #007bff;
                color: white;
                border: none;
                border-radius: 3px;
                cursor: pointer;
            }
            button:hover {
                background: #0056b3;
            }
            .users-header {
                font-weight: bold;
                margin-bottom: 15px;
                color: #333;
            }
            .user {
                padding: 8px;
                margin: 5px 0;
                background: #f5f5f5;
                border-radius: 3px;
                font-size: 14px;
            }
            .user.online::before {
                content: "● ";
                color: #28a745;
            }
            .user.offline::before {
                content: "○ ";
                color: #999;
            }
            .user.typing::before {
                content: "✎ ";
                color: #ffc107;
            }
            .status {
                font-size: 0.8em;
                color: #666;
                margin-top: 10px;
            }
        </style>
    </head>
    <body>
        <h1>💬 Chat Room</h1>

        <input type="text" id="nameInput" placeholder="Your name" value="Guest">
        <button onclick="connect()">Connect</button>
        <div class="status" id="status">Disconnected</div>

        <div class="container" id="chat" style="display:none;">
            <div class="sidebar">
                <div class="users-header">Users Online</div>
                <div id="usersList"></div>
            </div>

            <div class="chat">
                <div class="messages" id="messages"></div>
                <div class="input-area">
                    <input
                        type="text"
                        id="messageInput"
                        placeholder="Type message..."
                        onkeypress="if(event.key==='Enter')send()"
                    >
                    <button onclick="send()">Send</button>
                </div>
            </div>
        </div>

        <script>
        let ws = null;
        let userId = null;
        let userName = null;

        function log(message) {
            const el = document.getElementById("messages");
            const msg = document.createElement("div");
            msg.className = "message";
            msg.innerHTML = `<div class="time">${new Date().toLocaleTimeString()}</div><div>${message}</div>`;
            el.appendChild(msg);
            el.scrollTop = el.scrollHeight;
        }

        function updateStatus(text) {
            document.getElementById("status").textContent = text;
        }

        function connect() {
            userName = document.getElementById("nameInput").value;
            if (!userName) return;

            userId = "user_" + Math.random().toString(36).substr(2, 9);

            ws = new WebSocket(`ws://localhost:8000/ws/${userId}/${encodeURIComponent(userName)}`);

            ws.onopen = () => {
                document.getElementById("chat").style.display = "flex";
                updateStatus("Connected as " + userName);
                log(`You joined as ${userName}`);

                // Subscribe to messages
                ws.send(JSON.stringify({
                    type: "subscribe",
                    id: "messages",
                    payload: {
                        query: "subscription { message { id author text timestamp } }"
                    }
                }));

                // Subscribe to presence
                ws.send(JSON.stringify({
                    type: "subscribe",
                    id: "presence",
                    payload: {
                        query: "subscription { userPresence { userId name status } }"
                    }
                }));
            };

            ws.onmessage = (event) => {
                const msg = JSON.parse(new TextDecoder().decode(event.data));

                if (msg.payload && msg.payload.data) {
                    const data = msg.payload.data;

                    if (data.message) {
                        const m = data.message;
                        log(`<span class="author">${m.author}</span>: ${m.text}`);
                    }

                    if (data.userPresence) {
                        const user = data.userPresence;
                        if (user.status === "online") {
                            log(`${user.name} came online`);
                        } else if (user.status === "offline") {
                            log(`${user.name} went offline`);
                        } else if (user.status === "typing") {
                            log(`${user.name} is typing...`);
                        }
                        updateUserList();
                    }
                }
            };

            ws.onerror = (e) => {
                updateStatus("Error: " + e);
            };

            ws.onclose = () => {
                document.getElementById("chat").style.display = "none";
                updateStatus("Disconnected");
            };
        }

        async function updateUserList() {
            try {
                const res = await fetch("/api/users");
                const data = await res.json();
                const list = document.getElementById("usersList");
                list.innerHTML = data.users
                    .map(u => `<div class="user ${u.status}">${u.name}</div>`)
                    .join("");
            } catch (e) {
                console.error(e);
            }
        }

        function send() {
            const input = document.getElementById("messageInput");
            const text = input.value;

            if (!text || !ws) return;

            ws.send(JSON.stringify({
                type: "message",
                text: text
            }));

            input.value = "";
        }

        document.getElementById("messageInput")?.addEventListener("input", () => {
            if (ws) {
                ws.send(JSON.stringify({
                    type: "typing",
                    typing: true
                }));
            }
        });

        updateUserList();
        setInterval(updateUserList, 5000);
        </script>
    </body>
    </html>
    """)


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
