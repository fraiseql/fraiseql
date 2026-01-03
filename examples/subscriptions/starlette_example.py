"""
Complete Starlette application with GraphQL subscriptions.

Demonstrates that subscriptions work with any ASGI framework, not just FastAPI.

Features:
- WebSocket endpoint for GraphQL subscriptions
- REST endpoint for publishing events
- Lightweight Starlette routing

Run:
    uvicorn starlette_example:app --reload

Test:
    curl -X POST "http://localhost:8000/api/users/online?user_id=1&name=Alice"
    Connect WebSocket to ws://localhost:8000/graphql/subscriptions
"""

import asyncio
import json
import uuid
from datetime import datetime

from starlette.applications import Starlette
from starlette.routing import Route, WebSocketRoute
from starlette.responses import JSONResponse
from starlette.websockets import WebSocket
from fraiseql.subscriptions import SubscriptionManager
from fraiseql import _fraiseql_rs

# Initialize subscription manager
config = _fraiseql_rs.PyEventBusConfig.memory()
manager = SubscriptionManager(config)


# ============================================================================
# Resolver Functions
# ============================================================================

async def user_resolver(event: dict, variables: dict) -> dict:
    """Transform user event into subscription response."""
    return {
        "user": {
            "id": str(event.get("id")),
            "name": event.get("name"),
            "status": event.get("status", "unknown"),
        }
    }


async def message_resolver(event: dict, variables: dict) -> dict:
    """Transform message event into subscription response."""
    return {
        "message": {
            "id": str(event.get("id")),
            "text": event.get("text"),
            "author": event.get("author"),
        }
    }


# ============================================================================
# WebSocket Handler
# ============================================================================

async def graphql_subscriptions_endpoint(websocket: WebSocket):
    """GraphQL subscription WebSocket endpoint."""
    await websocket.accept()

    subscriptions = {}
    connection_id = str(uuid.uuid4())

    print(f"Client connected: {connection_id}")

    try:
        while True:
            # Receive message
            data = await websocket.receive_text()
            message = json.loads(data)
            message_type = message.get("type")

            if message_type == "connection_init":
                await websocket.send_text(json.dumps({
                    "type": "connection_ack"
                }))

            elif message_type == "subscribe":
                subscription_id = message.get("id")
                query = message["payload"]["query"]
                variables = message["payload"].get("variables", {})

                try:
                    resolver_fn = user_resolver if "user" in query else message_resolver

                    await manager.create_subscription(
                        subscription_id=subscription_id,
                        connection_id=connection_id,
                        query=query,
                        variables=variables,
                        resolver_fn=resolver_fn,
                        user_id="starlette_user",
                        tenant_id="starlette_tenant"
                    )

                    subscriptions[subscription_id] = True

                    await websocket.send_text(json.dumps({
                        "type": "next",
                        "id": subscription_id,
                        "payload": {"data": {}}
                    }))

                    # Start polling for events
                    asyncio.create_task(
                        poll_subscription_events(websocket, subscription_id)
                    )

                except Exception as e:
                    await websocket.send_text(json.dumps({
                        "type": "error",
                        "id": subscription_id,
                        "payload": {"message": str(e)}
                    }))

            elif message_type == "complete":
                subscription_id = message.get("id")
                await manager.complete_subscription(subscription_id)
                subscriptions.pop(subscription_id, None)

                await websocket.send_text(json.dumps({
                    "type": "complete",
                    "id": subscription_id
                }))

    except Exception as e:
        print(f"WebSocket error: {e}")

    finally:
        for sub_id in list(subscriptions.keys()):
            await manager.complete_subscription(sub_id)
        print(f"Client disconnected: {connection_id}")


async def poll_subscription_events(websocket: WebSocket, subscription_id: str):
    """Poll for events and send to client."""
    try:
        while True:
            response = await manager.get_next_event(subscription_id)
            if response:
                await websocket.send_bytes(response)
            else:
                await asyncio.sleep(0.01)
    except Exception as e:
        print(f"Polling error: {e}")


# ============================================================================
# REST Endpoints
# ============================================================================

async def user_online_endpoint(request):
    """Handle user online event."""
    user_id = request.query_params.get("user_id")
    name = request.query_params.get("name")

    await manager.publish_event(
        event_type="userOnline",
        channel="users",
        data={
            "id": str(uuid.uuid4()),
            "user_id": user_id,
            "name": name,
            "status": "online",
            "timestamp": datetime.now().isoformat()
        }
    )

    return JSONResponse({
        "status": "published",
        "user": name
    })


async def user_offline_endpoint(request):
    """Handle user offline event."""
    user_id = request.query_params.get("user_id")
    name = request.query_params.get("name")

    await manager.publish_event(
        event_type="userOffline",
        channel="users",
        data={
            "id": str(uuid.uuid4()),
            "user_id": user_id,
            "name": name,
            "status": "offline",
            "timestamp": datetime.now().isoformat()
        }
    )

    return JSONResponse({
        "status": "published",
        "user": name
    })


async def send_message_endpoint(request):
    """Handle send message event."""
    author = request.query_params.get("author")
    text = request.query_params.get("text")

    await manager.publish_event(
        event_type="messagePosted",
        channel="messages",
        data={
            "id": str(uuid.uuid4()),
            "author": author,
            "text": text,
            "timestamp": datetime.now().isoformat()
        }
    )

    return JSONResponse({
        "status": "published",
        "author": author,
        "text": text
    })


async def health_endpoint(request):
    """Health check."""
    return JSONResponse({"status": "healthy"})


# ============================================================================
# Starlette App
# ============================================================================

routes = [
    # WebSocket
    WebSocketRoute("/graphql/subscriptions", graphql_subscriptions_endpoint),

    # REST endpoints
    Route("/api/users/online", user_online_endpoint, methods=["POST"]),
    Route("/api/users/offline", user_offline_endpoint, methods=["POST"]),
    Route("/api/messages", send_message_endpoint, methods=["POST"]),
    Route("/health", health_endpoint, methods=["GET"]),
]

app = Starlette(routes=routes)


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
