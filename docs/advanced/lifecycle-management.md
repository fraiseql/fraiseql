# Lifecycle Management

FraiseQL supports custom lifecycle management through FastAPI's lifespan feature. This allows you to initialize resources on startup and clean them up on shutdown.

## Default Lifecycle

By default, FraiseQL manages:
- Database connection pool creation and cleanup
- Authentication provider initialization
- Basic health checks

## Custom Lifespan

Add your own startup/shutdown logic using the `lifespan` parameter:

```python
from contextlib import asynccontextmanager
from fastapi import FastAPI
import redis.asyncio as redis
from fraiseql import create_fraiseql_app

@asynccontextmanager
async def custom_lifespan(app: FastAPI):
    """Custom application lifespan."""
    # Startup
    print("🚀 Starting application...")

    # Initialize Redis
    app.state.redis = await redis.from_url(
        "redis://localhost:6379",
        decode_responses=True
    )

    # Initialize background task queue
    app.state.task_queue = TaskQueue()
    await app.state.task_queue.start()

    # Initialize cache
    app.state.cache = Cache(app.state.redis)

    # Warm up cache
    await warm_up_cache(app.state.cache)

    print("✅ Application started successfully")

    yield  # Application runs

    # Shutdown
    print("👋 Shutting down application...")

    # Clean up resources
    await app.state.task_queue.stop()
    await app.state.redis.close()

    print("✅ Application shut down successfully")

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    lifespan=custom_lifespan,
)
```

## Common Patterns

### Background Tasks

```python
import asyncio
from typing import Any

class TaskQueue:
    """Simple background task queue."""

    def __init__(self):
        self.queue: asyncio.Queue = asyncio.Queue()
        self.workers: list[asyncio.Task] = []
        self.running = False

    async def start(self, num_workers: int = 3):
        """Start background workers."""
        self.running = True
        for i in range(num_workers):
            worker = asyncio.create_task(self._worker(f"worker-{i}"))
            self.workers.append(worker)

    async def stop(self):
        """Stop all workers."""
        self.running = False
        # Wait for workers to finish
        await asyncio.gather(*self.workers, return_exceptions=True)

    async def _worker(self, name: str):
        """Process tasks from queue."""
        while self.running:
            try:
                task = await asyncio.wait_for(
                    self.queue.get(),
                    timeout=1.0
                )
                await self._process_task(task)
            except asyncio.TimeoutError:
                continue

    async def _process_task(self, task: dict[str, Any]):
        """Process a single task."""
        # Your task processing logic
        pass

    async def add_task(self, task: dict[str, Any]):
        """Add task to queue."""
        await self.queue.put(task)

@asynccontextmanager
async def lifespan_with_tasks(app: FastAPI):
    # Initialize task queue
    app.state.tasks = TaskQueue()
    await app.state.tasks.start()

    yield

    # Stop task queue
    await app.state.tasks.stop()
```

### Cache Warming

```python
async def warm_up_cache(cache: Cache):
    """Pre-populate cache with frequently accessed data."""
    db = get_db_pool()

    # Cache user roles
    roles = await db.fetch_all("SELECT * FROM roles")
    for role in roles:
        await cache.set(f"role:{role['id']}", role, expire=3600)

    # Cache feature flags
    features = await db.fetch_all("SELECT * FROM feature_flags")
    feature_dict = {f["name"]: f["enabled"] for f in features}
    await cache.set("features", feature_dict, expire=300)

    print(f"✅ Warmed up cache with {len(roles)} roles and {len(features)} features")

@asynccontextmanager
async def lifespan_with_cache(app: FastAPI):
    # Create cache
    app.state.cache = Cache()

    # Warm up cache
    await warm_up_cache(app.state.cache)

    # Refresh cache periodically
    async def refresh_cache():
        while True:
            await asyncio.sleep(300)  # Every 5 minutes
            try:
                await warm_up_cache(app.state.cache)
            except Exception as e:
                print(f"❌ Cache refresh failed: {e}")

    refresh_task = asyncio.create_task(refresh_cache())

    yield

    # Cancel refresh task
    refresh_task.cancel()
    await app.state.cache.close()
```

### External Service Connections

```python
@asynccontextmanager
async def lifespan_with_services(app: FastAPI):
    """Initialize external service connections."""
    # Connect to services
    app.state.services = {
        "email": EmailService(api_key=os.getenv("EMAIL_API_KEY")),
        "sms": SMSService(api_key=os.getenv("SMS_API_KEY")),
        "storage": S3Storage(
            bucket=os.getenv("S3_BUCKET"),
            region=os.getenv("AWS_REGION")
        ),
        "search": ElasticsearchClient(
            hosts=[os.getenv("ELASTICSEARCH_URL")]
        )
    }

    # Verify connections
    for name, service in app.state.services.items():
        try:
            await service.health_check()
            print(f"✅ {name} service connected")
        except Exception as e:
            print(f"❌ {name} service failed: {e}")
            # Decide if this should fail startup

    yield

    # Close connections
    for service in app.state.services.values():
        if hasattr(service, "close"):
            await service.close()
```

### Scheduled Jobs

```python
from apscheduler.schedulers.asyncio import AsyncIOScheduler

@asynccontextmanager
async def lifespan_with_scheduler(app: FastAPI):
    """Setup scheduled jobs."""
    scheduler = AsyncIOScheduler()

    # Define jobs
    async def cleanup_old_sessions():
        db = get_db_pool()
        await db.execute(
            "DELETE FROM sessions WHERE expires_at < NOW()"
        )

    async def generate_reports():
        # Generate daily reports
        pass

    # Schedule jobs
    scheduler.add_job(
        cleanup_old_sessions,
        "interval",
        hours=1,
        id="cleanup_sessions"
    )

    scheduler.add_job(
        generate_reports,
        "cron",
        hour=2,
        minute=0,
        id="daily_reports"
    )

    # Start scheduler
    scheduler.start()
    app.state.scheduler = scheduler

    yield

    # Shutdown scheduler
    scheduler.shutdown(wait=True)
```

## Combining with FraiseQL's Lifecycle

FraiseQL automatically wraps your custom lifespan to ensure the database pool is always available:

```python
@asynccontextmanager
async def my_lifespan(app: FastAPI):
    # FraiseQL has already initialized the DB pool
    db = get_db_pool()

    # Your custom initialization
    result = await db.fetch_one("SELECT version()")
    print(f"Connected to PostgreSQL: {result['version']}")

    yield

    # Your custom cleanup
    print("Custom cleanup complete")

    # FraiseQL will handle DB pool cleanup

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    lifespan=my_lifespan,
)
```

## Error Handling

Handle startup errors gracefully:

```python
@asynccontextmanager
async def safe_lifespan(app: FastAPI):
    """Lifespan with error handling."""
    initialized_services = []

    try:
        # Try to initialize services
        for service_name in ["cache", "queue", "search"]:
            service = await initialize_service(service_name)
            setattr(app.state, service_name, service)
            initialized_services.append(service_name)
            print(f"✅ Initialized {service_name}")

        yield

    except Exception as e:
        print(f"❌ Startup failed: {e}")
        # Clean up any services that were initialized
        for service_name in initialized_services:
            service = getattr(app.state, service_name, None)
            if service and hasattr(service, "close"):
                await service.close()
        raise

    finally:
        # Cleanup always runs
        print("Cleaning up services...")
        for service_name in initialized_services:
            service = getattr(app.state, service_name, None)
            if service and hasattr(service, "close"):
                try:
                    await service.close()
                except Exception as e:
                    print(f"Error closing {service_name}: {e}")
```

## Best Practices

1. **Quick startup**: Keep initialization fast for better deployment experience
2. **Graceful shutdown**: Ensure all resources are properly cleaned up
3. **Error recovery**: Handle initialization failures gracefully
4. **Health checks**: Verify external services during startup
5. **Logging**: Log startup/shutdown events for debugging
6. **Timeouts**: Set timeouts for external service connections

## Testing with Custom Lifespan

```python
import pytest
from httpx import AsyncClient

@pytest.fixture
async def app_with_lifespan():
    """Test fixture with custom lifespan."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test_db",
        lifespan=custom_lifespan,
    )

    async with AsyncClient(app=app, base_url="http://test") as client:
        # App lifespan runs automatically
        yield client

    # Lifespan cleanup happens automatically

async def test_with_lifespan(app_with_lifespan):
    """Test that uses initialized resources."""
    response = await app_with_lifespan.get("/health")
    assert response.status_code == 200
```

## See Also

- [Context Customization](./context-customization.md) - Access lifespan resources in context
- [Configuration](../configuration.md) - Environment-based configuration
- [FastAPI Lifespan](https://fastapi.tiangolo.com/advanced/events/) - FastAPI documentation
