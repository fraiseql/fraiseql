# NATS Integration Guide for Fraisier

## Overview

Fraisier uses **NATS with JetStream** for event-driven coordination across multi-region deployments. This guide explains how to set up, configure, and use the NATS event bus for deployment orchestration.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Architecture](#architecture)
3. [Configuration](#configuration)
4. [Event Types](#event-types)
5. [Usage Examples](#usage-examples)
6. [Multi-Region Deployments](#multi-region-deployments)
7. [Troubleshooting](#troubleshooting)
8. [Best Practices](#best-practices)

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Python 3.10+
- Fraisier installed with NATS support

### Starting NATS with Docker Compose

```bash
# Copy environment template
cp .env.example .env

# Start all services including NATS
docker-compose up

# Verify NATS is running
curl http://localhost:8222/varz
```

### Accessing NATS

- **Client Port**: `nats://localhost:4222` (default credentials: fraisier/fraisier_password)
- **Monitoring Port**: `http://localhost:8222` (HTTP monitoring)

## Architecture

### Event Flow

```
┌─────────────────────────────┐
│  Deployment Provider        │
│  (Bare Metal, Docker, etc)  │
└──────────────┬──────────────┘
               │ emit_deployment_started()
               │ emit_health_check_passed()
               │ emit_metrics()
               ▼
┌─────────────────────────────┐
│  NATS Event Bus             │
│  (nats://nats:4222)         │
└──────────────┬──────────────┘
               │ publish to stream
               ▼
┌─────────────────────────────┐
│  JetStream Persistence      │
│  (Event Storage)            │
└──────────────┬──────────────┘
               │ subscribe & route
               ▼
┌─────────────────────────────┐
│  Event Subscriber Registry  │
│  (Handler Matching)         │
└──────────────┬──────────────┘
               │ execute handlers
               ▼
┌─────────────────────────────┐
│  Event Handlers             │
│  (Webhooks, Metrics, etc)   │
└─────────────────────────────┘
```

### Key Components

| Component | Purpose | Location |
|-----------|---------|----------|
| **NatsClient** | Low-level connection wrapper | `fraisier/nats/client.py` |
| **NatsEventBus** | High-level event publishing | `fraisier/nats/client.py` |
| **NatsEventProvider** | Provider mixin for event emission | `fraisier/nats/provider.py` |
| **EventSubscriberRegistry** | Event routing and handler management | `fraisier/nats/subscribers.py` |
| **Configuration** | Environment-based settings | `fraisier/nats/config.py` |

## Configuration

### Environment Variables

Create a `.env` file (copy from `.env.example`):

```bash
# NATS Connection
NATS_SERVERS=nats://nats:4222
NATS_USERNAME=fraisier
NATS_PASSWORD=fraisier_password
NATS_TIMEOUT=5
NATS_MAX_RECONNECT_ATTEMPTS=60
NATS_RECONNECT_TIME_WAIT=2

# Regional Configuration
NATS_REGION=default
DEPLOYMENT_REGIONS=default

# Stream Retention
NATS_DEPLOYMENT_EVENTS_RETENTION=720    # 30 days
NATS_HEALTH_EVENTS_RETENTION=168        # 7 days
NATS_DATABASE_EVENTS_RETENTION=720      # 30 days
NATS_METRICS_EVENTS_RETENTION=168       # 7 days

# Event Handlers
ENABLE_WEBHOOK_NOTIFICATIONS=true
ENABLE_METRICS_RECORDING=true
ENABLE_EVENT_LOGGING=true
```

### Configuration Loading in Code

```python
from fraisier.nats import get_nats_config, is_nats_enabled

# Check if NATS is configured
if is_nats_enabled():
    # Load complete configuration
    config = get_nats_config()

    # Access sub-configurations
    print(f"NATS Servers: {config.connection.servers}")
    print(f"Region: {config.regional.region}")
    print(f"Deployment retention: {config.streams.deployment_events_retention_hours}h")
```

### Multi-Region Setup

For deployments across multiple regions:

```bash
# In us-east-1
NATS_REGION=us-east-1
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1
INTER_REGION_TIMEOUT=45

# In us-west-2
NATS_REGION=us-west-2
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1
INTER_REGION_TIMEOUT=45

# All regions share same NATS cluster for coordination
NATS_SERVERS=nats://nats-cluster:4222
```

## Event Types

### Deployment Events

```python
from fraisier.nats import DeploymentEvents

# Event types
DeploymentEvents.TRIGGERED      # Deployment requested
DeploymentEvents.STARTED        # Deployment begun
DeploymentEvents.COMPLETED      # Deployment succeeded
DeploymentEvents.FAILED         # Deployment failed
DeploymentEvents.ROLLED_BACK    # Rolled back to previous version
DeploymentEvents.METRICS_RECORDED  # Metrics snapshot
```

### Health Check Events

```python
from fraisier.nats import HealthCheckEvents

# Event types
HealthCheckEvents.CHECK_STARTED   # Health check initiated
HealthCheckEvents.CHECK_PASSED    # Service is healthy
HealthCheckEvents.CHECK_FAILED    # Service is unhealthy
HealthCheckEvents.CHECK_TIMEOUT   # Health check timed out
HealthCheckEvents.CHECK_RETRIED   # Retry after failure
```

### Database Events

```python
from fraisier.nats import DatabaseEvents

# Event types for multi-database coordination
DatabaseEvents.MIGRATION_STARTED
DatabaseEvents.MIGRATION_COMPLETED
DatabaseEvents.MIGRATION_FAILED
DatabaseEvents.SCHEMA_CHANGED
# ... and more
```

### Metrics Events

```python
from fraisier.nats import MetricsEvents

# For performance monitoring
MetricsEvents.DEPLOYMENT_DURATION
MetricsEvents.ERROR_RATE
MetricsEvents.RESOURCE_USAGE
# ... and more
```

## Usage Examples

### Example 1: Basic Event Publishing

```python
from fraisier.nats import NatsClient, NatsEventBus
from fraisier.nats.config import get_nats_config
import asyncio

async def publish_deployment_event():
    # Get configuration
    config = get_nats_config()

    # Create client and event bus
    client = NatsClient(**config.connection.to_nats_client_kwargs())
    event_bus = NatsEventBus(client)

    # Connect
    await client.connect()

    # Publish deployment event
    await event_bus.publish_deployment_event(
        event_type="deployment.started",
        deployment_id="deploy_123",
        data={
            "service": "api",
            "version": "2.0.0",
            "strategy": "rolling",
            "timestamp": "2024-01-15T10:30:00Z",
        },
        region="us-east-1",
    )

    await client.disconnect()

# Run
asyncio.run(publish_deployment_event())
```

### Example 2: Event Subscription and Handling

```python
from fraisier.nats import (
    get_subscriber_registry,
    EventFilter,
    EventHandlers,
)
import asyncio

async def subscribe_to_deployment_events():
    registry = get_subscriber_registry()

    # Log all deployment events
    registry.register(
        EventHandlers.log_event,
        EventFilter(event_type="deployment.started"),
    )

    # Send webhook on deployment failure
    registry.register(
        EventHandlers.notify_on_failure,
        EventFilter(event_type="deployment.failed"),
        is_async=True,
    )

    # Record metrics
    registry.register(
        EventHandlers.create_metric_recorder("deployment.completed"),
        EventFilter(event_type="deployment.completed"),
    )

    print(f"Registered {registry.get_subscription_count()} handlers")

asyncio.run(subscribe_to_deployment_events())
```

### Example 3: Custom Event Handler

```python
from fraisier.nats import get_subscriber_registry, EventFilter
import asyncio

async def my_deployment_handler(event):
    """Custom handler for deployment events."""
    service = event.data.get("service")
    status = event.data.get("status")
    print(f"Service {service} deployment status: {status}")

    # Send notification
    if "failed" in event.event_type:
        await send_alert(f"Deployment failed for {service}")

async def setup_custom_handler():
    registry = get_subscriber_registry()

    # Register custom handler
    registry.register(
        my_deployment_handler,
        EventFilter(service="api"),
        is_async=True,
    )

    print("Custom handler registered")

asyncio.run(setup_custom_handler())
```

### Example 4: Provider Integration

```python
from fraisier.providers.bare_metal import BareMetalProvider
from fraisier.nats import NatsClient, NatsEventBus
from fraisier.nats.config import get_nats_config

async def deploy_with_events():
    # Setup NATS
    config = get_nats_config()
    client = NatsClient(**config.connection.to_nats_client_kwargs())
    event_bus = NatsEventBus(client)
    await client.connect()

    # Create provider with event bus
    provider = BareMetalProvider(
        config={
            "host": "deployment.example.com",
            "username": "deploy",
            "key_path": "/home/user/.ssh/deploy_key",
        },
        event_bus=event_bus,
        region=config.regional.region,
    )

    # Connect to target
    await provider.connect()

    # Deploy - events are automatically emitted
    # emit_deployment_started() called
    # emit_health_check_started/passed/failed() called
    # emit_deployment_completed() called

    await provider.disconnect()
    await client.disconnect()

# Run deployment
import asyncio
asyncio.run(deploy_with_events())
```

### Example 5: Multi-Region Event Routing

```python
from fraisier.nats import (
    get_subscriber_registry,
    EventFilter,
    EventHandlers,
)

async def regional_event_handling():
    registry = get_subscriber_registry()

    # Handler for US region events
    def handle_us_events(event):
        print(f"US Event: {event.event_type}")

    # Handler for EU region events
    def handle_eu_events(event):
        print(f"EU Event: {event.event_type}")

    # Register regional handlers
    registry.register(
        handle_us_events,
        EventFilter(region="us-east-1"),
    )

    registry.register(
        handle_eu_events,
        EventFilter(region="eu-west-1"),
    )

    print(f"Registered {registry.get_subscription_count()} regional handlers")
```

## Multi-Region Deployments

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    NATS Cluster (Shared)                    │
│         nats://nats-cluster-1:4222                          │
└─────────────────────────────────────────────────────────────┘
  │                          │                          │
  ▼                          ▼                          ▼
┌──────────────┐      ┌──────────────┐        ┌──────────────┐
│  US-EAST-1   │      │  US-WEST-2   │        │  EU-WEST-1   │
│ ┌──────────┐ │      │ ┌──────────┐ │        │ ┌──────────┐ │
│ │Fraisier  │ │      │ │Fraisier  │ │        │ │Fraisier  │ │
│ │Region: US│ │      │ │Region: US│ │        │ │Region: EU│ │
│ │-EAST-1   │ │      │ │-WEST-2   │ │        │ │-WEST-1   │ │
│ └──────────┘ │      │ └──────────┘ │        │ └──────────┘ │
└──────────────┘      └──────────────┘        └──────────────┘
   │ emit events        │ emit events           │ emit events
   └────────────────────┼───────────────────────┘
                        │
           Events tagged with region
           Routes to regional handlers
```

### Configuration for Multi-Region

```bash
# Region A (us-east-1)
NATS_REGION=us-east-1
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1
NATS_SERVERS=nats://nats-central:4222

# Region B (us-west-2)
NATS_REGION=us-west-2
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1
NATS_SERVERS=nats://nats-central:4222

# Region C (eu-west-1)
NATS_REGION=eu-west-1
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1
NATS_SERVERS=nats://nats-central:4222
```

### Regional Event Filtering

```python
from fraisier.nats import get_subscriber_registry, EventFilter

async def coordinate_multi_region_deployment():
    registry = get_subscriber_registry()

    # US region handler
    def us_handler(event):
        print(f"[US] Processing: {event.event_type}")

    # EU region handler
    def eu_handler(event):
        print(f"[EU] Processing: {event.event_type}")

    # Register with regional filters
    registry.register(us_handler, EventFilter(region="us-east-1"))
    registry.register(eu_handler, EventFilter(region="eu-west-1"))
```

## Troubleshooting

### NATS Connection Issues

**Problem**: `ConnectionError: Failed to connect to NATS`

**Solutions**:

1. Verify NATS is running: `docker-compose ps nats`
2. Check NATS_SERVERS is correct: `echo $NATS_SERVERS`
3. Verify network connectivity: `curl http://localhost:8222/varz`
4. Check credentials: `NATS_USERNAME` and `NATS_PASSWORD`

### Event Not Being Received

**Problem**: Published events not reaching handlers

**Solutions**:

1. Verify event filter matches: Check EventFilter conditions
2. Check subscriber registration: `registry.get_subscription_count()`
3. Review NATS stream subscriptions
4. Check event type names match exactly
5. Enable debug logging: `FRAISIER_LOG_LEVEL=DEBUG`

### JetStream Storage Full

**Problem**: `Error: stream limit exceeded`

**Solutions**:

1. Check disk space: `df -h /data/nats`
2. Adjust retention: Update `NATS_*_EVENTS_RETENTION` env vars
3. Reduce max stream size: Update `NATS_STREAM_MAX_SIZE`
4. Purge old events: `nats stream purge <stream-name>`

### Event Handlers Not Executing

**Problem**: Handlers registered but not called

**Solutions**:

1. Verify handler registration: `registry.get_subscriptions_for_event_type("event.type")`
2. Check async/sync mismatch: Ensure `is_async=True` for async handlers
3. Review handler exceptions: Check logs for errors
4. Test handler directly: Call handler with test event

### NATS CLI Commands

```bash
# Access NATS CLI
docker-compose exec nats /bin/sh

# List streams
nats stream list

# View stream info
nats stream info DEPLOYMENT_EVENTS

# Purge stream
nats stream purge DEPLOYMENT_EVENTS

# View stream statistics
nats stream report
```

## Best Practices

### 1. Always Use Configuration Management

```python
# ✅ GOOD: Load from environment
from fraisier.nats import get_nats_config
config = get_nats_config()

# ❌ BAD: Hardcode servers
client = NatsClient(servers=["nats://localhost:4222"])
```

### 2. Handle Missing Event Bus Gracefully

```python
# ✅ GOOD: Check if NATS is enabled
from fraisier.nats import is_nats_enabled

if is_nats_enabled():
    event_bus = initialize_nats()
else:
    event_bus = None

provider = MyProvider(event_bus=event_bus)

# ❌ BAD: Assume NATS is always available
provider = MyProvider(event_bus=initialize_nats())  # May fail
```

### 3. Use Specific Event Filters

```python
# ✅ GOOD: Specific filters reduce noise
registry.register(
    handler,
    EventFilter(service="api", region="us-east-1"),
)

# ❌ BAD: Receive all events
registry.register(handler)  # Processes unnecessary events
```

### 4. Implement Retry Logic for Critical Handlers

```python
# ✅ GOOD: Retry on failure
registry.register(
    critical_handler,
    retry_on_failure=True,
    retry_count=3,
    retry_delay=2.0,
)

# ❌ BAD: Fail on first error
registry.register(critical_handler, retry_on_failure=False)
```

### 5. Monitor Event Processing

```python
# ✅ GOOD: Track handler execution
import logging
logger = logging.getLogger(__name__)

def monitored_handler(event):
    logger.info(f"Processing: {event.event_type}")
    # ... handler logic ...

# ❌ BAD: Silent execution
def handler(event):
    # No logging or monitoring
    pass
```

### 6. Clean Up Subscriptions

```python
# ✅ GOOD: Unregister when done
sub_id = registry.register(handler)
# ... use handler ...
registry.unregister(sub_id)

# ❌ BAD: Leave subscriptions registered
registry.register(handler)  # Accumulates subscriptions
```

### 7. Use Regional Configuration

```bash
# ✅ GOOD: Identify region
NATS_REGION=us-east-1
DEPLOYMENT_REGIONS=us-east-1,us-west-2,eu-west-1

# ❌ BAD: No regional awareness
# Events mixed across regions without filtering
```

### 8. Set Appropriate Retention Periods

```bash
# ✅ GOOD: Graduated retention
NATS_DEPLOYMENT_EVENTS_RETENTION=720      # 30 days - important
NATS_HEALTH_EVENTS_RETENTION=168          # 7 days - frequent
NATS_METRICS_EVENTS_RETENTION=168         # 7 days - high volume

# ❌ BAD: Same retention for all
# Wasting storage on high-volume events
```

## Production Deployment

### Security Considerations

1. **Enable TLS**:

   ```bash
   NATS_TLS_ENABLED=true
   NATS_TLS_CERT_FILE=/etc/nats/certs/server.crt
   NATS_TLS_KEY_FILE=/etc/nats/certs/server.key
   ```

2. **Use NKey Authentication**:

   ```bash
   # Instead of basic username/password
   NATS_NKey_SEED=SUAXXX...
   ```

3. **Enable Authorization Rules**:
   - Configure NATS with subject-based permissions
   - Restrict publish/subscribe by topic

### HA Setup

```yaml
# docker-compose.prod.yml
nats:
  image: nats:2.10-alpine
  command:
    - -js
    - -c
    - /etc/nats/nats-cluster.conf
  # Cluster with 3+ nodes for quorum
```

### Monitoring

- Monitor JetStream storage usage
- Track event processing latency
- Alert on connection drops
- Monitor handler error rates

## Support

For issues or questions:

1. Check [Troubleshooting](#troubleshooting) section
2. Review test cases in `tests/test_nats_*.py`
3. Check NATS server logs: `docker-compose logs nats`
4. Review Fraisier logs: `docker-compose logs fraisier`

## References

- [NATS Documentation](https://docs.nats.io/)
- [NATS JetStream Guide](https://docs.nats.io/nats-concepts/jetstream)
- [Fraisier Source Code](../fraisier/nats/)
- [Configuration Module](../fraisier/nats/config.py)
- [Test Examples](../tests/test_nats_*.py)
