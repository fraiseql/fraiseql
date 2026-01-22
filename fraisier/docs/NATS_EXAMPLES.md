# NATS Integration Examples

Complete working examples for using NATS event bus in Fraisier deployments.

## Table of Contents

1. [Basic Event Publishing](#basic-event-publishing)
2. [Event Subscription](#event-subscription)
3. [Provider Integration](#provider-integration)
4. [Advanced Scenarios](#advanced-scenarios)
5. [Multi-Region Coordination](#multi-region-coordination)
6. [Webhook Integration](#webhook-integration)
7. [Metrics and Monitoring](#metrics-and-monitoring)

## Basic Event Publishing

### Example: Publishing a Deployment Started Event

```python
import asyncio
from fraisier.nats import NatsClient, NatsEventBus
from fraisier.nats.config import get_nats_config

async def publish_deployment_event():
    """Publish a deployment started event."""
    # Load configuration from environment
    config = get_nats_config()

    # Initialize NATS client
    client = NatsClient(**config.connection.to_nats_client_kwargs())

    # Create event bus
    event_bus = NatsEventBus(client)

    try:
        # Connect to NATS
        await client.connect()
        print("Connected to NATS")

        # Publish deployment event
        await event_bus.publish_deployment_event(
            event_type="deployment.started",
            deployment_id="deploy_001",
            data={
                "service": "api",
                "version": "2.1.0",
                "strategy": "rolling",
                "timestamp": "2024-01-15T10:30:00Z",
                "deployed_by": "automation",
            },
            region=config.regional.region,
        )
        print("✓ Deployment started event published")

        # Publish health check started event
        await event_bus.publish_health_check_event(
            event_type="health_check.started",
            service_name="api",
            data={
                "check_type": "http",
                "endpoint": "http://localhost:8000/health",
                "timestamp": "2024-01-15T10:30:05Z",
            },
        )
        print("✓ Health check started event published")

        # Simulate health check passing
        await asyncio.sleep(2)

        await event_bus.publish_health_check_event(
            event_type="health_check.passed",
            service_name="api",
            data={
                "check_type": "http",
                "duration_ms": 150,
                "status_code": 200,
                "timestamp": "2024-01-15T10:30:07Z",
            },
        )
        print("✓ Health check passed event published")

        # Publish deployment completed event
        await event_bus.publish_deployment_event(
            event_type="deployment.completed",
            deployment_id="deploy_001",
            data={
                "service": "api",
                "status": "success",
                "duration_seconds": 120.5,
                "version": "2.1.0",
                "timestamp": "2024-01-15T10:32:00Z",
            },
        )
        print("✓ Deployment completed event published")

    finally:
        await client.disconnect()
        print("Disconnected from NATS")

# Run the example
if __name__ == "__main__":
    asyncio.run(publish_deployment_event())
```

## Event Subscription

### Example: Subscribing to Events with Handlers

```python
import asyncio
import logging
from fraisier.nats import (
    get_subscriber_registry,
    EventFilter,
    EventHandlers,
)

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

async def setup_event_handlers():
    """Register event handlers for deployment events."""
    registry = get_subscriber_registry()

    # Handler 1: Log all deployment events
    registry.register(
        EventHandlers.log_event,
        EventFilter(event_type="deployment.started"),
    )

    # Handler 2: Custom handler for successful deployments
    def on_deployment_success(event):
        service = event.data.get("service")
        version = event.data.get("version")
        duration = event.data.get("duration_seconds")
        logger.info(
            f"✓ Deployment successful: {service} v{version} in {duration}s"
        )

    registry.register(
        on_deployment_success,
        EventFilter(event_type="deployment.completed"),
    )

    # Handler 3: Async handler for deployment failures
    async def on_deployment_failure(event):
        service = event.data.get("service")
        error = event.data.get("error", "Unknown error")
        logger.error(f"✗ Deployment failed: {service}")
        logger.error(f"  Error: {error}")

        # Send notification to ops team
        await send_slack_notification(
            f"Deployment of {service} failed: {error}"
        )

    registry.register(
        on_deployment_failure,
        EventFilter(event_type="deployment.failed"),
        is_async=True,
    )

    # Handler 4: Track health checks
    def track_health_checks(event):
        service = event.data.get("service")
        event_type = event.event_type
        if "passed" in event_type:
            logger.info(f"✓ Health check passed: {service}")
        elif "failed" in event_type:
            logger.warning(f"✗ Health check failed: {service}")

    registry.register(
        track_health_checks,
        EventFilter(event_type=None),  # All health check events
    )

    logger.info(f"Registered {registry.get_subscription_count()} handlers")
    return registry

async def send_slack_notification(message):
    """Send notification to Slack (placeholder)."""
    logger.info(f"Would send Slack notification: {message}")

async def main():
    """Setup handlers and wait for events."""
    registry = await setup_event_handlers()
    logger.info("Event handlers ready, listening for events...")

    # Keep running
    try:
        while True:
            await asyncio.sleep(1)
    except KeyboardInterrupt:
        logger.info("Shutting down")

if __name__ == "__main__":
    asyncio.run(main())
```

## Provider Integration

### Example: Deploy with Automatic Event Emission

```python
import asyncio
from fraisier.providers.bare_metal import BareMetalProvider
from fraisier.nats import NatsClient, NatsEventBus
from fraisier.nats.config import get_nats_config
from fraisier.providers.base import HealthCheck, HealthCheckType

async def deploy_service_with_events():
    """Deploy a service with automatic event emission."""
    # Load NATS configuration
    nats_config = get_nats_config()

    # Initialize NATS
    nats_client = NatsClient(**nats_config.connection.to_nats_client_kwargs())
    event_bus = NatsEventBus(nats_client)
    await nats_client.connect()

    # Create deployment provider with event bus
    provider = BareMetalProvider(
        config={
            "host": "api-server.example.com",
            "port": 22,
            "username": "deploy",
            "key_path": "/home/user/.ssh/deploy_key",
        },
        event_bus=event_bus,
        region=nats_config.regional.region,
    )

    try:
        # Connect to target server
        print("Connecting to deployment target...")
        await provider.connect()

        # Deploy application
        print("Starting deployment...")
        exit_code, stdout, stderr = await provider.execute_command(
            "cd /opt/api && ./deploy.sh v2.1.0"
        )

        if exit_code == 0:
            print("✓ Deployment script executed successfully")

            # Run health check
            print("Running health checks...")
            health_check = HealthCheck(
                type=HealthCheckType.HTTP,
                url="http://localhost:8000/health",
                timeout=10,
                retries=3,
                retry_delay=2,
                service="api",
            )

            # Events are automatically emitted by provider:
            # - emit_health_check_started()
            # - emit_health_check_passed() if successful
            # - emit_health_check_failed() if failed
            is_healthy = await provider.check_health(health_check)

            if is_healthy:
                print("✓ Service is healthy, deployment complete")
            else:
                print("✗ Service health check failed, initiating rollback")
                await provider.execute_command("cd /opt/api && ./rollback.sh")
        else:
            print(f"✗ Deployment failed: {stderr}")

    finally:
        await provider.disconnect()
        await nats_client.disconnect()

if __name__ == "__main__":
    asyncio.run(deploy_service_with_events())
```

## Advanced Scenarios

### Example: Complex Event Processing with Retries

```python
import asyncio
import logging
from fraisier.nats import get_subscriber_registry, EventFilter

logger = logging.getLogger(__name__)

async def handle_critical_failure(event):
    """Handle critical deployment failures with retries."""
    service = event.data.get("service")
    error = event.data.get("error")

    logger.error(f"Critical failure detected: {service} - {error}")

    # Step 1: Attempt automated recovery
    logger.info(f"Attempting automated recovery for {service}...")
    recovered = await attempt_recovery(service)

    if recovered:
        logger.info(f"✓ Automatic recovery successful for {service}")
        return

    # Step 2: Escalate to manual intervention
    logger.warning(f"Automatic recovery failed, escalating {service}")
    await escalate_to_ops(service, error)

    # Step 3: Schedule retry
    logger.info(f"Scheduling retry for {service} in 5 minutes")
    await schedule_retry(service, delay=300)

async def attempt_recovery(service):
    """Attempt automated recovery."""
    # Restart service
    # Check logs for errors
    # Validate configuration
    return True  # Or False if recovery failed

async def escalate_to_ops(service, error):
    """Escalate issue to operations team."""
    logger.warning(f"Creating incident for {service}: {error}")

async def schedule_retry(service, delay):
    """Schedule deployment retry."""
    await asyncio.sleep(delay)
    logger.info(f"Retrying deployment for {service}")

async def setup_critical_handlers():
    """Setup handlers with retry logic."""
    registry = get_subscriber_registry()

    # Register with retry on failure
    registry.register(
        handle_critical_failure,
        EventFilter(event_type="deployment.failed"),
        is_async=True,
        retry_on_failure=True,
        retry_count=3,
        retry_delay=5.0,
    )

if __name__ == "__main__":
    asyncio.run(setup_critical_handlers())
```

## Multi-Region Coordination

### Example: Coordinated Multi-Region Deployment

```python
import asyncio
import logging
from fraisier.nats import (
    get_subscriber_registry,
    EventFilter,
    NatsClient,
    NatsEventBus,
)
from fraisier.nats.config import get_nats_config

logger = logging.getLogger(__name__)

class MultiRegionDeploymentCoordinator:
    """Coordinate deployments across multiple regions."""

    def __init__(self):
        self.region_deployments = {}
        self.event_bus = None

    async def initialize(self):
        """Initialize the coordinator."""
        config = get_nats_config()
        client = NatsClient(**config.connection.to_nats_client_kwargs())
        self.event_bus = NatsEventBus(client)
        await client.connect()

    async def start_regional_deployment(self, region, service, version):
        """Start deployment in a specific region."""
        if region not in self.region_deployments:
            self.region_deployments[region] = {}

        self.region_deployments[region][service] = {
            "version": version,
            "status": "in_progress",
        }

        logger.info(f"Starting deployment in {region}: {service} v{version}")

    async def on_regional_deployment_complete(self, event):
        """Handle deployment completion in a region."""
        region = event.region
        service = event.data.get("service")
        status = event.data.get("status")

        if region in self.region_deployments:
            self.region_deployments[region][service]["status"] = status
            logger.info(f"Region {region} deployment complete: {status}")

            # Check if all regions are done
            if self.all_regions_done():
                logger.info("✓ All regions deployed successfully!")

    def all_regions_done(self):
        """Check if all regions have completed deployment."""
        for region_data in self.region_deployments.values():
            for service_data in region_data.values():
                if service_data["status"] == "in_progress":
                    return False
        return True

async def multi_region_example():
    """Example of coordinated multi-region deployment."""
    coordinator = MultiRegionDeploymentCoordinator()
    await coordinator.initialize()

    config = get_nats_config()
    registry = get_subscriber_registry()

    # Register handler for each region
    for region in config.regional.all_regions:
        registry.register(
            coordinator.on_regional_deployment_complete,
            EventFilter(
                event_type="deployment.completed",
                region=region,
            ),
            is_async=True,
        )

    # Start deployments in all regions
    for region in config.regional.all_regions:
        await coordinator.start_regional_deployment(
            region, "api", "2.1.0"
        )

    logger.info(
        f"Deployed to {len(config.regional.all_regions)} regions: "
        f"{', '.join(config.regional.all_regions)}"
    )

if __name__ == "__main__":
    asyncio.run(multi_region_example())
```

## Webhook Integration

### Example: Send Webhooks on Deployment Events

```python
import asyncio
import aiohttp
import logging
from fraisier.nats import get_subscriber_registry, EventFilter
from fraisier.nats.config import get_nats_config

logger = logging.getLogger(__name__)

class WebhookNotifier:
    """Send webhooks for deployment events."""

    def __init__(self, webhook_url):
        self.webhook_url = webhook_url
        self.session = None

    async def initialize(self):
        """Initialize HTTP session."""
        self.session = aiohttp.ClientSession()

    async def shutdown(self):
        """Close HTTP session."""
        if self.session:
            await self.session.close()

    async def send_webhook(self, event):
        """Send webhook notification."""
        try:
            payload = {
                "event_type": event.event_type,
                "data": event.data,
                "region": event.region,
                "timestamp": event.data.get("timestamp"),
            }

            async with self.session.post(
                self.webhook_url,
                json=payload,
                timeout=aiohttp.ClientTimeout(total=10),
            ) as response:
                if response.status == 200:
                    logger.info(
                        f"✓ Webhook sent: {event.event_type}"
                    )
                else:
                    logger.error(
                        f"✗ Webhook failed: {response.status} "
                        f"- {await response.text()}"
                    )

        except asyncio.TimeoutError:
            logger.error("✗ Webhook timeout")
        except Exception as e:
            logger.error(f"✗ Webhook error: {e}")

async def setup_webhook_notifications():
    """Setup webhook notifications."""
    config = get_nats_config()

    if not config.handlers.deployment_webhook_url:
        logger.warning("No webhook URL configured")
        return

    notifier = WebhookNotifier(config.handlers.deployment_webhook_url)
    await notifier.initialize()

    registry = get_subscriber_registry()

    # Send webhook for all deployment events
    registry.register(
        notifier.send_webhook,
        EventFilter(event_type=None),  # All events
        is_async=True,
    )

    logger.info(f"Webhook notifications enabled: {config.handlers.deployment_webhook_url}")

if __name__ == "__main__":
    asyncio.run(setup_webhook_notifications())
```

## Metrics and Monitoring

### Example: Collect Deployment Metrics

```python
import asyncio
import time
import logging
from fraisier.nats import get_subscriber_registry, EventFilter

logger = logging.getLogger(__name__)

class DeploymentMetrics:
    """Collect and track deployment metrics."""

    def __init__(self):
        self.total_deployments = 0
        self.successful_deployments = 0
        self.failed_deployments = 0
        self.deployment_durations = []
        self.health_check_durations = []

    def record_deployment_start(self, event):
        """Record deployment start."""
        self.total_deployments += 1
        logger.info(
            f"Deployment started (total: {self.total_deployments}): "
            f"{event.data.get('service')}"
        )

    def record_deployment_complete(self, event):
        """Record deployment completion."""
        status = event.data.get("status")
        duration = event.data.get("duration_seconds", 0)

        if status == "success":
            self.successful_deployments += 1
        else:
            self.failed_deployments += 1

        self.deployment_durations.append(duration)

        success_rate = (
            self.successful_deployments / self.total_deployments * 100
            if self.total_deployments > 0
            else 0
        )

        logger.info(
            f"Deployment complete: {status} | "
            f"Duration: {duration}s | "
            f"Success rate: {success_rate:.1f}%"
        )

    def record_health_check(self, event):
        """Record health check metrics."""
        duration_ms = event.data.get("duration_ms", 0)
        self.health_check_durations.append(duration_ms)

        avg_duration = (
            sum(self.health_check_durations) / len(self.health_check_durations)
            if self.health_check_durations
            else 0
        )

        logger.info(
            f"Health check: {duration_ms}ms (avg: {avg_duration:.0f}ms)"
        )

    def get_summary(self):
        """Get metrics summary."""
        success_rate = (
            self.successful_deployments / self.total_deployments * 100
            if self.total_deployments > 0
            else 0
        )

        avg_duration = (
            sum(self.deployment_durations) / len(self.deployment_durations)
            if self.deployment_durations
            else 0
        )

        return {
            "total_deployments": self.total_deployments,
            "successful": self.successful_deployments,
            "failed": self.failed_deployments,
            "success_rate_percent": success_rate,
            "avg_duration_seconds": avg_duration,
            "health_checks": len(self.health_check_durations),
        }

async def setup_metrics_collection():
    """Setup metrics collection."""
    metrics = DeploymentMetrics()
    registry = get_subscriber_registry()

    # Track deployment events
    registry.register(
        metrics.record_deployment_start,
        EventFilter(event_type="deployment.started"),
    )

    registry.register(
        metrics.record_deployment_complete,
        EventFilter(event_type="deployment.completed"),
    )

    # Track health checks
    registry.register(
        metrics.record_health_check,
        EventFilter(event_type="health_check.passed"),
    )

    # Periodically log summary
    async def log_summary():
        while True:
            await asyncio.sleep(300)  # Every 5 minutes
            summary = metrics.get_summary()
            logger.info(f"Deployment metrics: {summary}")

    asyncio.create_task(log_summary())

    logger.info("Metrics collection enabled")

if __name__ == "__main__":
    asyncio.run(setup_metrics_collection())
```

## Complete Application Example

```python
"""
Complete example: Deployment orchestration with NATS events
"""

import asyncio
import logging
from fraisier.nats import (
    NatsClient,
    NatsEventBus,
    get_subscriber_registry,
    EventFilter,
    get_nats_config,
)
from fraisier.providers.bare_metal import BareMetalProvider
from fraisier.providers.base import HealthCheck, HealthCheckType

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

async def main():
    """Complete deployment workflow with event coordination."""

    # 1. Load configuration
    config = get_nats_config()
    logger.info(f"Loaded NATS configuration for region: {config.regional.region}")

    # 2. Initialize NATS
    nats_client = NatsClient(**config.connection.to_nats_client_kwargs())
    event_bus = NatsEventBus(nats_client)
    await nats_client.connect()
    logger.info("Connected to NATS event bus")

    # 3. Setup event handlers
    registry = get_subscriber_registry()

    def on_deployment_event(event):
        logger.info(f"Event: {event.event_type} - {event.data.get('service')}")

    registry.register(on_deployment_event)

    # 4. Create provider with event emission
    provider = BareMetalProvider(
        config={
            "host": "example.com",
            "username": "deploy",
            "key_path": "/home/deploy/.ssh/id_rsa",
        },
        event_bus=event_bus,
        region=config.regional.region,
    )

    try:
        await provider.connect()

        # 5. Deploy service
        logger.info("Starting deployment...")
        await provider.start_service("api")

        # 6. Run health checks
        health_check = HealthCheck(
            type=HealthCheckType.HTTP,
            url="http://localhost:8000/health",
            timeout=10,
            retries=3,
            retry_delay=2,
            service="api",
        )
        await provider.check_health(health_check)

        logger.info("Deployment completed successfully!")

    finally:
        await provider.disconnect()
        await nats_client.disconnect()

if __name__ == "__main__":
    asyncio.run(main())
```

## Testing Examples

For comprehensive test examples, see:
- `tests/test_nats_config.py` - Configuration tests
- `tests/test_nats_integration.py` - Integration tests
- `tests/test_subscribers.py` - Subscriber tests
