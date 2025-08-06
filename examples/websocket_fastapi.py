"""Example of WebSocket subscription endpoint with FastAPI."""

import asyncio
from collections.abc import AsyncGenerator
from datetime import UTC
from uuid import UUID, uuid4

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse

import fraiseql
from fraiseql import subscription
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.subscriptions import SubscriptionManager


# Define types
@fraiseql.type
class Task:
    id: UUID
    title: str
    status: str
    created_at: str


# Define a query (required by GraphQL)
@fraiseql.query
async def current_time(info) -> str:
    """Get current server time."""
    from datetime import datetime

    return datetime.now(UTC).isoformat()


# Define subscriptions
@subscription
async def task_feed(info) -> AsyncGenerator[Task]:
    """Subscribe to new tasks as they're created."""
    for i in range(10):
        await asyncio.sleep(1)  # Simulate real-time updates

        task = Task(
            id=uuid4(),
            title=f"Task {i + 1}",
            status="pending",
            created_at=f"2025-01-19T10:{i:02d}:00Z",
        )

        yield task


# Create FastAPI app
app = FastAPI()

# Create subscription manager
subscription_manager = SubscriptionManager()

# Build GraphQL schema
schema = build_fraiseql_schema(subscription_resolvers=[task_feed])

# Set schema on manager
subscription_manager.schema = schema


@app.get("/")
async def get():
    """Serve a simple WebSocket client page."""
    return HTMLResponse("""
<!DOCTYPE html>
<html>
<head>
    <title>FraiseQL WebSocket Subscriptions</title>
</head>
<body>
    <h1>FraiseQL WebSocket Subscriptions Demo</h1>
    <div id="status">Disconnected</div>
    <button onclick="connect()">Connect</button>
    <button onclick="subscribe()">Subscribe to Tasks</button>
    <button onclick="disconnect()">Disconnect</button>
    <div id="messages"></div>

    <script>
        let ws = null;
        let subId = "sub1";

        function connect() {
            ws = new WebSocket("ws://localhost:8000/graphql-ws");

            ws.onopen = () => {
                document.getElementById("status").textContent = "Connected";
                addMessage("Connected to server");

                // Send connection_init
                ws.send(JSON.stringify({
                    type: "connection_init",
                    payload: {}
                }));
            };

            ws.onmessage = (event) => {
                const message = JSON.parse(event.data);
                console.log("Received:", message);

                if (message.type === "connection_ack") {
                    addMessage("Connection acknowledged");
                } else if (message.type === "data" && message.id === subId) {
                    const task = message.payload.data.task_feed;
                    addMessage(`New task: ${task.title} (${task.status})`);
                } else if (message.type === "complete") {
                    addMessage("Subscription completed");
                } else if (message.type === "error") {
                    addMessage(`Error: ${JSON.stringify(message.payload)}`);
                }
            };

            ws.onclose = () => {
                document.getElementById("status").textContent = "Disconnected";
                addMessage("Disconnected from server");
            };
        }

        function subscribe() {
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                alert("Please connect first");
                return;
            }

            const query = `
                subscription {
                    task_feed {
                        id
                        title
                        status
                        created_at
                    }
                }
            `;

            ws.send(JSON.stringify({
                id: subId,
                type: "subscribe",
                payload: {
                    query: query
                }
            }));

            addMessage("Subscribed to task feed");
        }

        function disconnect() {
            if (ws) {
                ws.close();
            }
        }

        function addMessage(text) {
            const messages = document.getElementById("messages");
            const msg = document.createElement("div");
            msg.textContent = new Date().toLocaleTimeString() + " - " + text;
            messages.appendChild(msg);
            messages.scrollTop = messages.scrollHeight;
        }
    </script>
</body>
</html>
    """)


@app.websocket("/graphql-ws")
async def graphql_ws_endpoint(websocket: WebSocket):
    """WebSocket endpoint for GraphQL subscriptions."""
    # Accept connection with subprotocol
    subprotocol = None
    if "graphql-ws" in websocket.headers.get("sec-websocket-protocol", "").split(", "):
        subprotocol = "graphql-ws"
    elif "graphql-transport-ws" in websocket.headers.get(
        "sec-websocket-protocol",
        "",
    ).split(", "):
        subprotocol = "graphql-transport-ws"

    await websocket.accept(subprotocol=subprotocol)

    # Add connection to manager
    connection = await subscription_manager.add_connection(
        websocket,
        subprotocol=subprotocol,
        context={"websocket": websocket},
    )

    try:
        # Handle connection
        await connection.handle()
    except WebSocketDisconnect:
        pass
    finally:
        # Remove connection
        await subscription_manager.remove_connection(connection.connection_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
