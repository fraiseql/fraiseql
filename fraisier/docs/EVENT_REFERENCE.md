# Fraisier Event Reference

**Version**: 0.1.0
**Transport**: NATS with JetStream persistence
**Format**: JSON with structured metadata

This document describes all events emitted by Fraisier through the NATS event bus. Use these events to integrate Fraisier with external systems, build custom dashboards, trigger workflows, and maintain audit logs.

---

## Quick Start

### Subscribe to Events

```python
from fraisier.nats import EventSubscriberRegistry, EventFilter, get_event_bus

# Create registry
registry = EventSubscriberRegistry()

# Define handler
def on_deployment_complete(event):
    print(f"Deployment {event.data['deployment_id']} complete: {event.data['status']}")

# Register for events
registry.register(
    on_deployment_complete,
    EventFilter(event_type="deployment.completed")
)

# Start listening
await registry.listen()
```

### Subscribe via NATS CLI

```bash
# Subscribe to all deployment events
nats sub "fraisier.deployment.>"

# Subscribe to completed deployments only
nats sub "fraisier.deployment.completed.>"

# Subscribe to specific region
nats sub "fraisier.deployment.completed.us-east-1"

# Subscribe to all events
nats sub "fraisier.>"
```

---

## Event Subjects

All events are published to NATS subjects following this pattern:

```
fraisier.[event_type].[region]
```

**Examples**:
- `fraisier.deployment.started.us-east-1`
- `fraisier.health_check.passed.us-west-2`
- `fraisier.deployment.failed.default`

### Subject Hierarchy

```
fraisier
├── deployment
│   ├── started
│   ├── completed
│   ├── failed
│   ├── cancelled
│   └── rolled_back
├── health_check
│   ├── started
│   ├── passed
│   └── failed
└── metrics
    ├── deployment
    └── service
```

---

## Event Structure

All events share a common structure:

```json
{
  "event_id": "evt_00001",
  "event_type": "deployment.started",
  "timestamp": "2024-01-22T10:00:00Z",
  "region": "us-east-1",
  "trace_id": "trace_abc123def456",
  "source": "fraisier-prod-1",
  "data": {
    // Event-specific data
  }
}
```

**Top-Level Fields**:
- `event_id` (string): Unique event identifier
- `event_type` (string): Type of event (e.g., deployment.started)
- `timestamp` (ISO 8601): When event occurred
- `region` (string): AWS/deployment region (default: "default")
- `trace_id` (string): Distributed trace ID for tracking across systems
- `source` (string): Which Fraisier instance emitted event
- `data` (object): Event-specific payload

---

## Deployment Events

### deployment.started

Emitted when a deployment begins.

**Subject**: `fraisier.deployment.started.{region}`

**Data Fields**:
```json
{
  "event_type": "deployment.started",
  "deployment_id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "version": "2.0.0",
  "previous_version": "1.9.0",
  "strategy": "rolling",
  "strategy_config": {
    "max_instances_down": 1,
    "health_check_delay": 10,
    "health_check_timeout": 30
  },
  "triggered_by": "user_123",
  "trigger_type": "api",
  "provider": "bare_metal",
  "instances_count": 4,
  "metadata": {
    "ticket": "DEPLOY-123",
    "reason": "Feature release"
  }
}
```

**Typical Handlers**:
- Log deployment start
- Notify Slack/Discord
- Create monitoring dashboard
- Trigger pre-deployment backup

---

### deployment.completed

Emitted when a deployment finishes successfully.

**Subject**: `fraisier.deployment.completed.{region}`

**Data Fields**:
```json
{
  "event_type": "deployment.completed",
  "deployment_id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "status": "success",
  "version": "2.0.0",
  "previous_version": "1.9.0",
  "strategy": "rolling",
  "duration_seconds": 330,
  "triggered_by": "user_123",
  "provider": "bare_metal",
  "instances_total": 4,
  "instances_updated": 4,
  "instances_failed": 0,
  "health_checks": {
    "status": "passing",
    "checks_passed": 3,
    "checks_total": 3,
    "last_check": "2024-01-22T10:05:25Z"
  },
  "metrics": {
    "success_rate": 99.95,
    "error_rate": 0.05,
    "latency_p50_ms": 120,
    "latency_p95_ms": 250,
    "latency_p99_ms": 350,
    "throughput_rps": 1200
  }
}
```

**Typical Handlers**:
- Send success notification
- Record deployment in database
- Update dashboard
- Trigger post-deployment tests
- Archive deployment logs

---

### deployment.failed

Emitted when a deployment fails.

**Subject**: `fraisier.deployment.failed.{region}`

**Data Fields**:
```json
{
  "event_type": "deployment.failed",
  "deployment_id": "dep_00002",
  "fraise": "my_api",
  "environment": "production",
  "status": "failed",
  "version": "2.0.0",
  "previous_version": "1.9.0",
  "strategy": "rolling",
  "duration_seconds": 60,
  "triggered_by": "user_123",
  "provider": "bare_metal",
  "failed_at_stage": "health_check",
  "error_code": "HEALTH_CHECK_FAILED",
  "error_message": "Health check failed: connection refused",
  "error_details": {
    "endpoint": "http://localhost:8000/health",
    "timeout_ms": 30000,
    "response_status": null,
    "response_body": null
  },
  "instances_total": 4,
  "instances_updated": 1,
  "instances_failed": 3,
  "rollback_performed": true,
  "rollback_status": "success",
  "rollback_to_version": "1.9.0",
  "rollback_duration_seconds": 120
}
```

**Typical Handlers**:
- Send critical alert (Slack, PagerDuty)
- Create incident in issue tracker
- Trigger automatic rollback (if not already done)
- Notify on-call engineer
- Archive logs for post-mortem

---

### deployment.cancelled

Emitted when a deployment is cancelled.

**Subject**: `fraisier.deployment.cancelled.{region}`

**Data Fields**:
```json
{
  "event_type": "deployment.cancelled",
  "deployment_id": "dep_00003",
  "fraise": "my_api",
  "environment": "staging",
  "version": "2.1.0",
  "previous_version": "2.0.0",
  "strategy": "rolling",
  "duration_seconds": 180,
  "triggered_by": "user_123",
  "cancelled_by": "user_456",
  "cancellation_reason": "User cancelled deployment",
  "instances_updated": 2,
  "instances_total": 4,
  "rollback_performed": true,
  "rollback_status": "success"
}
```

---

### deployment.rolled_back

Emitted when a deployment is rolled back.

**Subject**: `fraisier.deployment.rolled_back.{region}`

**Data Fields**:
```json
{
  "event_type": "deployment.rolled_back",
  "deployment_id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "from_version": "2.0.0",
  "to_version": "1.9.0",
  "rollback_type": "automatic",
  "rollback_reason": "High error rate (5%)",
  "rollback_initiated_by": "health_check",
  "original_deployment_id": "dep_00001",
  "original_deployment_duration": 60,
  "rollback_duration_seconds": 120,
  "rolled_back_at": "2024-01-22T10:07:00Z",
  "instances_rolled_back": 4,
  "metrics_at_rollback": {
    "error_rate": 5.0,
    "success_rate": 95.0,
    "latency_p99_ms": 5000
  }
}
```

---

## Health Check Events

### health_check.started

Emitted when health checks begin after deployment.

**Subject**: `fraisier.health_check.started.{region}`

**Data Fields**:
```json
{
  "event_type": "health_check.started",
  "deployment_id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "check_type": "http",
  "endpoint": "http://localhost:8000/health",
  "check_count": 3,
  "check_timeout_seconds": 30,
  "check_interval_seconds": 5
}
```

---

### health_check.passed

Emitted when health checks pass.

**Subject**: `fraisier.health_check.passed.{region}`

**Data Fields**:
```json
{
  "event_type": "health_check.passed",
  "deployment_id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "check_type": "http",
  "endpoint": "http://localhost:8000/health",
  "duration_ms": 50,
  "checks_passed": 3,
  "checks_total": 3,
  "response_time_ms": 45,
  "http_status": 200,
  "response_body_size": 256
}
```

---

### health_check.failed

Emitted when health checks fail.

**Subject**: `fraisier.health_check.failed.{region}`

**Data Fields**:
```json
{
  "event_type": "health_check.failed",
  "deployment_id": "dep_00002",
  "fraise": "my_api",
  "environment": "production",
  "check_type": "http",
  "endpoint": "http://localhost:8000/health",
  "duration_ms": 5000,
  "checks_passed": 0,
  "checks_total": 3,
  "failure_reason": "Connection refused",
  "error_code": "CONNECTION_REFUSED",
  "http_status": null,
  "will_retry": true,
  "retry_in_seconds": 5,
  "retry_attempt": 1,
  "retry_attempts_total": 3
}
```

---

## Metrics Events

### metrics.deployment

Emitted after deployment with performance metrics.

**Subject**: `fraisier.metrics.deployment.{region}`

**Data Fields**:
```json
{
  "event_type": "metrics.deployment",
  "deployment_id": "dep_00001",
  "fraise": "my_api",
  "environment": "production",
  "duration_seconds": 330,
  "instances": 4,
  "deployment_timing": {
    "pre_deployment_checks_seconds": 5,
    "backup_seconds": 10,
    "deployment_seconds": 280,
    "health_checks_seconds": 30,
    "post_deployment_seconds": 5
  },
  "service_metrics": {
    "cpu_usage_percent": 45.2,
    "memory_usage_mb": 512,
    "disk_usage_percent": 75,
    "network_bandwidth_mbps": 125
  },
  "application_metrics": {
    "requests_total": 45000,
    "requests_failed": 225,
    "error_rate": 0.5,
    "success_rate": 99.5,
    "latency_p50_ms": 120,
    "latency_p95_ms": 250,
    "latency_p99_ms": 350,
    "latency_p999_ms": 450,
    "throughput_rps": 1200,
    "availability_percent": 99.95
  }
}
```

---

### metrics.service

Emitted periodically with service health metrics.

**Subject**: `fraisier.metrics.service.{region}`

**Data Fields**:
```json
{
  "event_type": "metrics.service",
  "fraise": "my_api",
  "environment": "production",
  "timestamp": "2024-01-22T10:58:30Z",
  "instances": {
    "total": 4,
    "healthy": 4,
    "degraded": 0,
    "unhealthy": 0
  },
  "database": {
    "connections": 45,
    "max_connections": 100,
    "connection_pool_exhausted": false,
    "query_latency_p99_ms": 150
  },
  "application": {
    "uptime_seconds": 86400,
    "requests_total": 10000000,
    "error_rate": 0.01,
    "success_rate": 99.99,
    "latency_p99_ms": 200
  }
}
```

---

## Event Filtering

### Filter by Event Type

```python
from fraisier.nats import EventFilter, EventSubscriberRegistry

registry = EventSubscriberRegistry()

# Only deployment.started events
registry.register(handler, EventFilter(event_type="deployment.started"))

# All deployment events
registry.register(handler, EventFilter(event_type="deployment.*"))

# All events
registry.register(handler, EventFilter())
```

### Filter by Service

```python
# Only my_api events
registry.register(handler, EventFilter(service="my_api"))

# Multiple services
registry.register(handler, EventFilter(service=["my_api", "my_worker"]))
```

### Filter by Environment

```python
# Only production events
registry.register(handler, EventFilter(environment="production"))

# Multiple environments
registry.register(handler, EventFilter(environment=["staging", "production"]))
```

### Filter by Region

```python
# Only us-east-1 events
registry.register(handler, EventFilter(region="us-east-1"))

# All regions
registry.register(handler, EventFilter(region="*"))
```

### Filter by Status

```python
# Only failed deployments
registry.register(
    handler,
    EventFilter(event_type="deployment.failed")
)

# Only successful deployments
registry.register(
    handler,
    EventFilter(event_type="deployment.completed")
)
```

### Compound Filters

```python
# Only production deployment completions
registry.register(
    handler,
    EventFilter(
        event_type="deployment.completed",
        environment="production"
    )
)

# All deployment events in production for my_api
registry.register(
    handler,
    EventFilter(
        event_type="deployment.*",
        environment="production",
        service="my_api"
    )
)
```

---

## Event Replay

With JetStream persistence, you can replay events:

```bash
# Replay events from 1 hour ago
nats stream view DEPLOYMENT_EVENTS --samples 1000 --since "-1h"

# Replay all events for a specific service
nats stream view DEPLOYMENT_EVENTS --filter "fraisier.deployment.*" --samples 10000
```

### Programmatic Replay

```python
from fraisier.nats import get_event_bus

event_bus = await get_event_bus()

# Get events from the last 24 hours
events = await event_bus.get_events_since(
    start_time=time.time() - 86400,  # 24 hours ago
    event_types=["deployment.started", "deployment.completed"]
)

for event in events:
    print(f"{event.event_type}: {event.data['deployment_id']}")
```

---

## Event Examples

### Example 1: Complete Deployment Lifecycle

```json
// 1. deployment.started
{
  "event_id": "evt_001",
  "event_type": "deployment.started",
  "timestamp": "2024-01-22T10:00:00Z",
  "data": {
    "deployment_id": "dep_00001",
    "fraise": "my_api",
    "environment": "production",
    "version": "2.0.0"
  }
}

// 2. health_check.started
{
  "event_id": "evt_002",
  "event_type": "health_check.started",
  "timestamp": "2024-01-22T10:00:05Z",
  "data": {
    "deployment_id": "dep_00001",
    "check_type": "http",
    "check_count": 3
  }
}

// 3. health_check.passed
{
  "event_id": "evt_003",
  "event_type": "health_check.passed",
  "timestamp": "2024-01-22T10:05:00Z",
  "data": {
    "deployment_id": "dep_00001",
    "checks_passed": 3,
    "duration_ms": 50
  }
}

// 4. deployment.completed
{
  "event_id": "evt_004",
  "event_type": "deployment.completed",
  "timestamp": "2024-01-22T10:05:30Z",
  "data": {
    "deployment_id": "dep_00001",
    "status": "success",
    "duration_seconds": 330
  }
}

// 5. metrics.deployment
{
  "event_id": "evt_005",
  "event_type": "metrics.deployment",
  "timestamp": "2024-01-22T10:05:35Z",
  "data": {
    "deployment_id": "dep_00001",
    "error_rate": 0.01,
    "latency_p99_ms": 150
  }
}
```

### Example 2: Failed Deployment with Rollback

```json
// 1. deployment.started
{
  "event_id": "evt_101",
  "event_type": "deployment.started",
  "timestamp": "2024-01-22T11:00:00Z",
  "data": {
    "deployment_id": "dep_00002",
    "version": "2.1.0",
    "environment": "production"
  }
}

// 2. health_check.failed
{
  "event_id": "evt_102",
  "event_type": "health_check.failed",
  "timestamp": "2024-01-22T11:00:30Z",
  "data": {
    "deployment_id": "dep_00002",
    "checks_passed": 0,
    "failure_reason": "Connection refused"
  }
}

// 3. deployment.failed
{
  "event_id": "evt_103",
  "event_type": "deployment.failed",
  "timestamp": "2024-01-22T11:00:35Z",
  "data": {
    "deployment_id": "dep_00002",
    "error_code": "HEALTH_CHECK_FAILED",
    "rollback_performed": true
  }
}

// 4. deployment.rolled_back
{
  "event_id": "evt_104",
  "event_type": "deployment.rolled_back",
  "timestamp": "2024-01-22T11:02:00Z",
  "data": {
    "deployment_id": "dep_00002",
    "from_version": "2.1.0",
    "to_version": "2.0.0",
    "rollback_status": "success"
  }
}
```

---

## Integration Patterns

### Build Custom Dashboards

```python
# Consume events to build real-time dashboard
async def update_dashboard():
    registry = EventSubscriberRegistry()

    def on_any_event(event):
        # Update dashboard state
        emit_to_websocket({
            "event": event.event_type,
            "data": event.data,
            "timestamp": event.timestamp
        })

    registry.register(on_any_event, EventFilter())
```

### Trigger External Workflows

```python
# Trigger CI/CD pipeline on deployment success
def on_deployment_complete(event):
    if event.data['status'] == 'success':
        # Trigger smoke tests
        requests.post(
            f"https://ci.example.com/trigger",
            json={
                "service": event.data['fraise'],
                "version": event.data['version']
            }
        )

registry.register(on_deployment_complete, EventFilter(event_type="deployment.completed"))
```

### Build Audit Logs

```python
# Log all events to audit system
def audit_event(event):
    db.insert('audit_log', {
        'event_type': event.event_type,
        'deployment_id': event.data.get('deployment_id'),
        'user': event.data.get('triggered_by'),
        'timestamp': event.timestamp,
        'details': json.dumps(event.data)
    })

registry.register(audit_event, EventFilter())
```

### Alert on Failures

```python
# Send alerts on deployment failures
def alert_on_failure(event):
    if event.data['error_code'] == 'HEALTH_CHECK_FAILED':
        send_alert({
            "service": event.data['fraise'],
            "environment": event.data['environment'],
            "reason": event.data['error_message'],
            "deployment_id": event.data['deployment_id']
        })

registry.register(alert_on_failure, EventFilter(event_type="deployment.failed"))
```

---

## Event Retention

Events are retained in JetStream according to configuration:

- **Deployment Events**: 720 hours (30 days, default)
- **Health Check Events**: 168 hours (7 days, default)
- **Metrics Events**: 168 hours (7 days, default)

Configure retention:
```bash
export NATS_DEPLOYMENT_EVENTS_RETENTION=1440  # 60 days
```

---

## See Also

- [NATS_INTEGRATION_GUIDE.md](NATS_INTEGRATION_GUIDE.md) - Event bus setup
- [WEBHOOK_REFERENCE.md](WEBHOOK_REFERENCE.md) - Webhook events
- [API_REFERENCE.md](API_REFERENCE.md) - HTTP event endpoints
