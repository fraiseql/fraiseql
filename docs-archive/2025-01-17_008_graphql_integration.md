# Beta Development Log: Sprint 1 - GraphQL Subscription Integration
**Date**: 2025-01-17
**Time**: 09:00 UTC
**Session**: 008
**Author**: Backend Lead (Viktor had his coffee today)

## Day 4: GraphQL Schema Integration

### Extending Schema Builder for Subscriptions

#### Created: `/src/fraiseql/decorators/subscription.py`
```python
"""Subscription decorator for GraphQL subscriptions."""

import asyncio
import inspect
from typing import Any, Callable, TypeVar, AsyncGenerator
from functools import wraps

from fraiseql.core.registry import get_registry
from fraiseql.core.types import SubscriptionField

F = TypeVar("F", bound=Callable[..., Any])


def subscription(fn: F) -> F:
    """
    Decorator to mark a function as a GraphQL subscription.

    Example:
        @subscription
        async def task_updates(info, project_id: UUID) -> AsyncGenerator[Task, None]:
            async for task in watch_project_tasks(project_id):
                yield task
    """
    if not inspect.isasyncgenfunction(fn):
        raise TypeError(
            f"Subscription {fn.__name__} must be an async generator function "
            f"(use 'async def' and 'yield')"
        )

    # Extract type hints
    hints = inspect.get_annotations(fn)
    return_type = hints.get("return", Any)

    # Parse AsyncGenerator type
    if hasattr(return_type, "__origin__") and return_type.__origin__ is AsyncGenerator:
        yield_type = return_type.__args__[0] if return_type.__args__ else Any
    else:
        # Try to infer from first yield
        yield_type = Any

    # Create subscription field
    field = SubscriptionField(
        name=fn.__name__,
        resolver=fn,
        return_type=yield_type,
        args=hints,
        description=fn.__doc__
    )

    # Register with global registry
    registry = get_registry()
    registry.register_subscription(field)

    # Add metadata
    fn._is_subscription = True
    fn._field_def = field

    return fn
```

#### Updated: `/src/fraiseql/gql/schema_builder.py`
```python
# Added subscription support to schema builder

class FraiseQLSchemaBuilder:
    """Enhanced schema builder with subscription support."""

    def __init__(self):
        self._types: dict[str, GraphQLObjectType] = {}
        self._queries: dict[str, GraphQLField] = {}
        self._mutations: dict[str, GraphQLField] = {}
        self._subscriptions: dict[str, GraphQLField] = {}  # NEW

    def register_subscription(self, sub_fn: Callable[..., AsyncGenerator]) -> None:
        """Register a subscription function."""
        field_name = sub_fn.__name__

        # Convert to GraphQL field
        field = GraphQLField(
            type_=self._get_graphql_type(sub_fn._field_def.return_type),
            args=self._build_args(sub_fn),
            subscribe=self._wrap_subscription_resolver(sub_fn),
            description=sub_fn.__doc__
        )

        self._subscriptions[field_name] = field

    def _wrap_subscription_resolver(self, sub_fn: Callable) -> Callable:
        """Wrap subscription function for GraphQL execution."""
        @wraps(sub_fn)
        async def subscribe(root, info, **kwargs):
            # Add subscription context
            info.context["is_subscription"] = True

            # Execute subscription
            async for value in sub_fn(info, **kwargs):
                yield {"data": {sub_fn.__name__: value}}

        return subscribe

    def build_schema(self) -> GraphQLSchema:
        """Build complete schema with subscriptions."""
        # ... existing query and mutation building ...

        # Build subscription type
        subscription_type = None
        if self._subscriptions:
            subscription_type = GraphQLObjectType(
                "Subscription",
                fields=self._subscriptions,
                description="Root subscription type"
            )

        return GraphQLSchema(
            query=query_type,
            mutation=mutation_type,
            subscription=subscription_type,
            types=list(self._types.values())
        )
```

### Subscription Examples

#### Created: `/examples/subscriptions/task_subscriptions.py`
```python
"""Example task management subscriptions."""

from typing import AsyncGenerator
from uuid import UUID
from datetime import datetime

from fraiseql import subscription, requires_auth
from models import Task, TaskEvent, TaskEventType


@subscription
@requires_auth
async def task_updates(
    info,
    project_id: UUID,
    event_types: list[TaskEventType] | None = None
) -> AsyncGenerator[TaskEvent, None]:
    """
    Subscribe to task updates for a project.

    Filters:
    - project_id: Project to watch
    - event_types: Optional list of event types to filter
    """
    user = info.context["user"]
    db = info.context["db"]

    # Verify user has access to project
    if not await db.user_has_project_access(user.user_id, project_id):
        raise PermissionError("No access to project")

    # Subscribe to PostgreSQL notifications
    channel = f"task_events_{project_id}"
    async with db.listen(channel) as listener:
        async for notification in listener:
            event = TaskEvent.parse_raw(notification.payload)

            # Filter by event types if specified
            if event_types and event.type not in event_types:
                continue

            # Yield event to subscriber
            yield event


@subscription
async def task_statistics(
    info,
    project_id: UUID,
    interval_seconds: int = 60
) -> AsyncGenerator[dict, None]:
    """
    Subscribe to real-time task statistics.

    Updates every `interval_seconds` with project statistics.
    """
    db = info.context["db"]

    while True:
        # Calculate statistics
        stats = await db.fetch_one("""
            SELECT
                COUNT(*) as total_tasks,
                COUNT(*) FILTER (WHERE status = 'completed') as completed,
                COUNT(*) FILTER (WHERE status = 'in_progress') as in_progress,
                COUNT(*) FILTER (WHERE due_date < CURRENT_DATE AND status != 'completed') as overdue,
                AVG(EXTRACT(EPOCH FROM (updated_at - created_at))) as avg_completion_time
            FROM tasks
            WHERE project_id = $1
        """, project_id)

        yield {
            "project_id": project_id,
            "timestamp": datetime.utcnow(),
            "total_tasks": stats["total_tasks"],
            "completed": stats["completed"],
            "in_progress": stats["in_progress"],
            "overdue": stats["overdue"],
            "avg_completion_time_seconds": stats["avg_completion_time"]
        }

        # Wait for next interval
        await asyncio.sleep(interval_seconds)


@subscription
@requires_auth
async def my_notifications(info) -> AsyncGenerator[Notification, None]:
    """Subscribe to user's real-time notifications."""
    user = info.context["user"]
    db = info.context["db"]
    registry = info.context["connection_registry"]

    # Register this subscription for direct notifications
    connection = info.context["connection"]
    connection.notification_channels.add(f"user_{user.user_id}")

    # Listen for notifications
    channel = f"notifications_{user.user_id}"
    async with db.listen(channel) as listener:
        async for notification in listener:
            data = json.loads(notification.payload)

            # Create notification object
            notif = Notification(
                id=data["id"],
                type=data["type"],
                title=data["title"],
                message=data["message"],
                created_at=datetime.fromisoformat(data["created_at"]),
                read=False
            )

            yield notif
```

### PostgreSQL Integration

#### Created: `/src/fraiseql/subscriptions/postgres_pubsub.py`
```python
"""PostgreSQL LISTEN/NOTIFY integration for subscriptions."""

import asyncio
import json
from typing import AsyncGenerator, Any, Dict
from contextlib import asynccontextmanager

import asyncpg
from asyncpg import Connection, Pool


class PostgresListener:
    """Manages PostgreSQL LISTEN/NOTIFY for subscriptions."""

    def __init__(self, pool: Pool):
        self.pool = pool
        self._listeners: Dict[str, asyncio.Queue] = {}
        self._connection: Connection | None = None
        self._listening_task: asyncio.Task | None = None

    async def start(self):
        """Start the listener connection."""
        self._connection = await self.pool.acquire()
        self._listening_task = asyncio.create_task(self._listen_loop())

    async def stop(self):
        """Stop the listener."""
        if self._listening_task:
            self._listening_task.cancel()
            try:
                await self._listening_task
            except asyncio.CancelledError:
                pass

        if self._connection:
            await self.pool.release(self._connection)
            self._connection = None

    @asynccontextmanager
    async def listen(self, channel: str) -> AsyncGenerator[asyncio.Queue, None]:
        """
        Listen to a PostgreSQL channel.

        Usage:
            async with listener.listen("my_channel") as queue:
                async for notification in queue:
                    print(notification)
        """
        # Create queue for this listener
        queue = asyncio.Queue()

        # Register listener
        if channel not in self._listeners:
            self._listeners[channel] = []
            await self._connection.add_listener(channel, self._on_notification)

        self._listeners[channel].append(queue)

        try:
            yield queue
        finally:
            # Unregister listener
            self._listeners[channel].remove(queue)
            if not self._listeners[channel]:
                del self._listeners[channel]
                await self._connection.remove_listener(channel, self._on_notification)

    def _on_notification(self, connection, pid, channel, payload):
        """Handle incoming notification."""
        if channel in self._listeners:
            # Parse notification
            try:
                data = json.loads(payload)
            except json.JSONDecodeError:
                data = {"raw": payload}

            # Send to all listeners
            for queue in self._listeners[channel]:
                try:
                    queue.put_nowait(data)
                except asyncio.QueueFull:
                    # Queue full, skip this notification
                    pass

    async def _listen_loop(self):
        """Main listening loop."""
        try:
            while True:
                # Just keep the connection alive
                await asyncio.sleep(30)
                await self._connection.execute("SELECT 1")
        except asyncio.CancelledError:
            pass
        except Exception as e:
            print(f"Listener error: {e}")
            # Attempt to reconnect
            await self.stop()
            await self.start()


class PostgresPubSub:
    """High-level pub/sub interface for PostgreSQL."""

    def __init__(self, pool: Pool):
        self.pool = pool
        self.listener = PostgresListener(pool)

    async def publish(self, channel: str, message: Dict[str, Any]):
        """Publish message to channel."""
        payload = json.dumps(message)
        await self.pool.execute(
            "SELECT pg_notify($1, $2)",
            channel,
            payload
        )

    async def subscribe(self, channel: str) -> AsyncGenerator[Dict[str, Any], None]:
        """Subscribe to channel messages."""
        async with self.listener.listen(channel) as queue:
            while True:
                message = await queue.get()
                yield message

    async def start(self):
        """Start pub/sub system."""
        await self.listener.start()

    async def stop(self):
        """Stop pub/sub system."""
        await self.listener.stop()
```

### Testing GraphQL Subscriptions

#### Created: `/tests/subscriptions/test_graphql_integration.py`
```python
"""Test GraphQL subscription integration."""

import asyncio
import pytest
from uuid import uuid4

from fraiseql import create_fraiseql_app, subscription
from fraiseql.testing import GraphQLTestClient


@pytest.mark.asyncio
class TestGraphQLSubscriptions:

    @pytest.fixture
    async def app_with_subscriptions(self):
        """Create app with test subscriptions."""

        @subscription
        async def countdown(info, from_number: int = 10):
            """Count down from a number."""
            for i in range(from_number, 0, -1):
                yield {"value": i}
                await asyncio.sleep(0.1)
            yield {"value": 0, "message": "Blast off!"}

        @subscription
        async def random_numbers(info, count: int = 5):
            """Generate random numbers."""
            import random
            for _ in range(count):
                yield {"value": random.randint(1, 100)}
                await asyncio.sleep(0.1)

        app = create_fraiseql_app(
            database_url="postgresql://test/test",
            types=[]
        )

        return app

    async def test_subscription_execution(self, app_with_subscriptions):
        """Test basic subscription execution."""
        query = """
        subscription Countdown {
            countdown(fromNumber: 3) {
                value
                message
            }
        }
        """

        async with GraphQLTestClient(app_with_subscriptions) as client:
            # Subscribe
            subscription = await client.subscribe(query)

            # Collect results
            results = []
            async for result in subscription:
                results.append(result)

            # Verify countdown
            assert len(results) == 4  # 3, 2, 1, 0
            assert results[0]["data"]["countdown"]["value"] == 3
            assert results[-1]["data"]["countdown"]["value"] == 0
            assert results[-1]["data"]["countdown"]["message"] == "Blast off!"

    async def test_subscription_with_error(self):
        """Test subscription error handling."""

        @subscription
        async def failing_subscription(info):
            """Subscription that fails."""
            yield {"value": 1}
            raise Exception("Subscription error!")

        app = create_fraiseql_app(
            database_url="postgresql://test/test",
            types=[]
        )

        query = """
        subscription {
            failingSubscription {
                value
            }
        }
        """

        async with GraphQLTestClient(app) as client:
            subscription = await client.subscribe(query)

            # First result should succeed
            first = await subscription.__anext__()
            assert first["data"]["failingSubscription"]["value"] == 1

            # Should get error
            with pytest.raises(Exception):
                await subscription.__anext__()

    async def test_concurrent_subscriptions(self, app_with_subscriptions):
        """Test multiple concurrent subscriptions."""
        query1 = "subscription { countdown(fromNumber: 5) { value } }"
        query2 = "subscription { randomNumbers(count: 5) { value } }"

        async with GraphQLTestClient(app_with_subscriptions) as client:
            # Start two subscriptions
            sub1 = await client.subscribe(query1)
            sub2 = await client.subscribe(query2)

            # Collect results concurrently
            async def collect(sub, name):
                results = []
                async for result in sub:
                    results.append(result)
                return name, results

            results = await asyncio.gather(
                collect(sub1, "countdown"),
                collect(sub2, "random")
            )

            # Verify both completed
            countdown_results = next(r[1] for r in results if r[0] == "countdown")
            random_results = next(r[1] for r in results if r[0] == "random")

            assert len(countdown_results) == 6  # 5 to 0
            assert len(random_results) == 5
```

### Viktor's Morning Review

*Viktor walks in with design diagrams and coffee*

"Good morning! Let's see the GraphQL integration... *reviews code intensely*

EXCELLENT:
- Clean decorator API - even junior devs can use it
- PostgreSQL LISTEN/NOTIFY is the right choice
- Proper async generator handling
- Test coverage looking good

CONCERNS:
- Where's the subscription complexity analysis?
- No distributed pub/sub for scaling beyond one instance
- Missing subscription result caching
- Need subscription lifecycle hooks

IMMEDIATE TASKS:
1. Add complexity scoring for subscriptions
2. Implement Redis fallback for multi-instance
3. Add subscription filtering in resolver
4. Create subscription monitoring dashboard

Here's what I want to see by end of day:

```python
@subscription
@complexity(score=10)  # Prevent expensive subscriptions
@filter("project.is_public OR user.has_access")  # Declarative filtering
@cache(ttl=5)  # Cache subscription results
async def expensive_analytics(info, timeframe: str):
    # Your implementation
    pass
```

Also, start working on the DataLoader integration. I want to see N+1 query detection by tomorrow!"

*Pins a note: "Subscriptions: ✓ Good start. Keep momentum!"*

---
Next Log: Subscription complexity and filtering
