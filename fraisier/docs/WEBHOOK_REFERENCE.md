# Fraisier Webhook Reference

**Version**: 0.1.0

Webhooks allow Fraisier to notify external systems about events in your deployment pipeline. Use webhooks to integrate with Slack, Discord, PagerDuty, monitoring systems, and custom applications.

---

## Quick Start

```bash
# Register a webhook for deployment completions
fraisier webhook add \
  --event deployment.completed \
  https://example.com/webhook/fraisier

# Test the webhook
fraisier webhook test webhook_123

# View all webhooks
fraisier webhook list
```

---

## Webhook Configuration

### Register a Webhook

```bash
fraisier webhook add [OPTIONS] WEBHOOK_URL
```

**Options**:

- `--event EVENT`: Event type (repeatable, at least one required)
- `--secret SECRET`: Webhook secret for signature verification
- `--active / --inactive`: Enable/disable webhook (default: active)
- `--metadata KEY=VALUE`: Custom metadata (repeatable)
- `--retry-count N`: Number of retries on failure (default: 3)
- `--retry-delay SECONDS`: Delay between retries (default: 5)
- `--timeout SECONDS`: Request timeout (default: 30)

**Examples**:

```bash
# Single event
fraisier webhook add \
  --event deployment.completed \
  https://example.com/webhook

# Multiple events
fraisier webhook add \
  --event deployment.started \
  --event deployment.failed \
  --event health_check.failed \
  https://example.com/webhook

# With secret and metadata
fraisier webhook add \
  --event deployment.completed \
  --secret my_secret_key \
  --metadata team=platform \
  --metadata priority=high \
  https://example.com/webhook

# List all events
fraisier webhook add \
  --event deployment.started \
  --event deployment.completed \
  --event deployment.failed \
  --event deployment.cancelled \
  --event health_check.started \
  --event health_check.passed \
  --event health_check.failed \
  https://example.com/webhook/all-events
```

---

## Event Types

### deployment.started

Fired when a deployment begins.

**Payload**:

```json
{
  "event": "deployment.started",
  "event_id": "evt_00001",
  "timestamp": "2024-01-22T10:00:00Z",
  "data": {
    "deployment_id": "dep_00001",
    "fraise": "my_api",
    "environment": "production",
    "version": "2.0.0",
    "previous_version": "1.9.0",
    "strategy": "rolling",
    "triggered_by": "user_123",
    "trigger_type": "api",
    "metadata": {
      "ticket": "DEPLOY-123",
      "reason": "Feature release"
    }
  }
}
```

---

### deployment.completed

Fired when a deployment finishes successfully.

**Payload**:

```json
{
  "event": "deployment.completed",
  "event_id": "evt_00002",
  "timestamp": "2024-01-22T10:05:30Z",
  "data": {
    "deployment_id": "dep_00001",
    "fraise": "my_api",
    "environment": "production",
    "status": "success",
    "version": "2.0.0",
    "previous_version": "1.9.0",
    "strategy": "rolling",
    "duration_seconds": 330,
    "triggered_by": "user_123",
    "health_checks_passed": 3,
    "health_checks_total": 3,
    "metrics": {
      "success_rate": 100.0,
      "error_rate": 0.0,
      "latency_p99_ms": 150
    }
  }
}
```

---

### deployment.failed

Fired when a deployment fails.

**Payload**:

```json
{
  "event": "deployment.failed",
  "event_id": "evt_00003",
  "timestamp": "2024-01-22T10:06:00Z",
  "data": {
    "deployment_id": "dep_00002",
    "fraise": "my_api",
    "environment": "production",
    "status": "failed",
    "version": "2.0.0",
    "previous_version": "1.9.0",
    "strategy": "rolling",
    "duration_seconds": 60,
    "triggered_by": "user_123",
    "error_code": "HEALTH_CHECK_FAILED",
    "error_message": "Health check failed: connection refused",
    "failed_at_stage": "health_check",
    "health_checks_passed": 1,
    "health_checks_total": 3,
    "rollback_performed": true,
    "rollback_status": "success"
  }
}
```

---

### deployment.cancelled

Fired when a deployment is cancelled.

**Payload**:

```json
{
  "event": "deployment.cancelled",
  "event_id": "evt_00004",
  "timestamp": "2024-01-22T10:03:00Z",
  "data": {
    "deployment_id": "dep_00003",
    "fraise": "my_api",
    "environment": "staging",
    "status": "cancelled",
    "version": "2.1.0",
    "previous_version": "2.0.0",
    "duration_seconds": 180,
    "triggered_by": "user_123",
    "cancelled_by": "user_456",
    "cancellation_reason": "User cancelled"
  }
}
```

---

### deployment.rolled_back

Fired when a deployment is rolled back.

**Payload**:

```json
{
  "event": "deployment.rolled_back",
  "event_id": "evt_00005",
  "timestamp": "2024-01-22T10:07:00Z",
  "data": {
    "deployment_id": "dep_00001",
    "fraise": "my_api",
    "environment": "production",
    "status": "rolled_back",
    "from_version": "2.0.0",
    "to_version": "1.9.0",
    "rollback_reason": "High error rate (5%)",
    "rollback_initiated_by": "health_check",
    "original_deployment_id": "dep_00001",
    "rollback_duration_seconds": 120,
    "rolled_back_at": "2024-01-22T10:07:00Z"
  }
}
```

---

### health_check.started

Fired when health checks begin.

**Payload**:

```json
{
  "event": "health_check.started",
  "event_id": "evt_00006",
  "timestamp": "2024-01-22T10:00:30Z",
  "data": {
    "deployment_id": "dep_00001",
    "fraise": "my_api",
    "environment": "production",
    "check_type": "http",
    "endpoint": "http://localhost:8000/health",
    "check_count": 3,
    "check_timeout_seconds": 30
  }
}
```

---

### health_check.passed

Fired when health checks pass.

**Payload**:

```json
{
  "event": "health_check.passed",
  "event_id": "evt_00007",
  "timestamp": "2024-01-22T10:05:00Z",
  "data": {
    "deployment_id": "dep_00001",
    "fraise": "my_api",
    "environment": "production",
    "check_type": "http",
    "endpoint": "http://localhost:8000/health",
    "duration_ms": 50,
    "checks_passed": 3,
    "checks_total": 3,
    "response_time_ms": 45,
    "http_status": 200
  }
}
```

---

### health_check.failed

Fired when health checks fail.

**Payload**:

```json
{
  "event": "health_check.failed",
  "event_id": "evt_00008",
  "timestamp": "2024-01-22T10:00:40Z",
  "data": {
    "deployment_id": "dep_00002",
    "fraise": "my_api",
    "environment": "production",
    "check_type": "http",
    "endpoint": "http://localhost:8000/health",
    "duration_ms": 5000,
    "checks_passed": 0,
    "checks_total": 3,
    "reason": "Connection refused",
    "error_code": "CONNECTION_REFUSED",
    "http_status": null,
    "will_retry": true,
    "retry_in_seconds": 5
  }
}
```

---

## Webhook Security

### Signature Verification

All webhooks include a signature header for verification:

```
X-Fraisier-Signature: sha256=abcd1234...
```

**Generate Signature** (Python example):

```python
import hmac
import hashlib
import json

def verify_webhook_signature(payload, signature, secret):
    """Verify webhook signature."""
    # payload is the raw request body
    expected_sig = hmac.new(
        secret.encode(),
        payload,
        hashlib.sha256
    ).hexdigest()

    return hmac.compare_digest(
        f"sha256={expected_sig}",
        signature
    )
```

### Secret Management

1. Generate a secure secret:

```bash
python -c "import secrets; print(secrets.token_urlsafe(32))"
```

2. Store in environment variable (never in code):

```bash
export FRAISIER_WEBHOOK_SECRET="your_secret_here"
```

3. Verify signature on received webhook:

```python
signature = request.headers.get('X-Fraisier-Signature')
if not verify_webhook_signature(request.body, signature, os.getenv('FRAISIER_WEBHOOK_SECRET')):
    return 401, "Unauthorized"
```

---

## Webhook Delivery

### Retry Logic

Failed webhooks are retried with exponential backoff:

- **Retry 1**: Immediately
- **Retry 2**: 5 seconds later
- **Retry 3**: 25 seconds later (5 + 20)
- **Retry 4**: 125 seconds later (25 + 100)

After all retries fail, the webhook is marked as failed and logged.

### Request Format

All webhooks are sent as HTTP POST with:

- **Content-Type**: `application/json`
- **Timeout**: 30 seconds (configurable)
- **Retries**: 3 attempts (configurable)

### Response Handling

- **2xx**: Success - webhook marked as delivered
- **3xx**: Redirect - follows redirect (up to 5)
- **4xx**: Client error - no retry (webhook marked failed)
- **5xx**: Server error - retry with backoff
- **Timeout**: Retry with backoff

---

## Webhook Examples

### Slack Integration

```python
import hmac
import hashlib
import os
from flask import Flask, request

app = Flask(__name__)

def verify_signature(payload, signature):
    secret = os.getenv('FRAISIER_WEBHOOK_SECRET')
    expected = hmac.new(
        secret.encode(),
        payload,
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(f"sha256={expected}", signature)

@app.route('/webhook/fraisier', methods=['POST'])
def fraisier_webhook():
    # Verify signature
    signature = request.headers.get('X-Fraisier-Signature')
    if not verify_signature(request.data, signature):
        return 401, "Unauthorized"

    data = request.json
    event = data['event']
    deployment = data['data']

    # Build Slack message
    if event == 'deployment.started':
        message = {
            "text": f"🚀 Deployment Started",
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": f"*Deployment Started* → {deployment['fraise']}/{deployment['environment']}\n"
                                f"Version: {deployment['version']} (from {deployment['previous_version']})\n"
                                f"Strategy: {deployment['strategy']}"
                    }
                }
            ]
        }

    elif event == 'deployment.completed':
        message = {
            "text": f"✅ Deployment Successful",
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": f"*Deployment Complete* → {deployment['fraise']}/{deployment['environment']}\n"
                                f"Version: {deployment['version']}\n"
                                f"Duration: {deployment['duration_seconds']}s"
                    }
                }
            ]
        }

    elif event == 'deployment.failed':
        message = {
            "text": f"❌ Deployment Failed",
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": f"*Deployment Failed* → {deployment['fraise']}/{deployment['environment']}\n"
                                f"Error: {deployment['error_message']}\n"
                                f"Rollback: {'✅ Success' if deployment['rollback_performed'] else '⚠️ None'}"
                    }
                }
            ]
        }

    # Send to Slack
    slack_webhook = os.getenv('SLACK_WEBHOOK_URL')
    requests.post(slack_webhook, json=message)

    return 200, "OK"

if __name__ == '__main__':
    app.run(port=5000)
```

Register webhook:

```bash
fraisier webhook add \
  --event deployment.started \
  --event deployment.completed \
  --event deployment.failed \
  --secret $FRAISIER_WEBHOOK_SECRET \
  http://your-server.com/webhook/fraisier
```

### Discord Integration

```python
@app.route('/webhook/discord', methods=['POST'])
def discord_webhook():
    data = request.json
    event = data['event']
    deployment = data['data']

    # Build Discord embed
    if event == 'deployment.completed':
        embed = {
            "title": f"✅ {deployment['fraise']} Deployed",
            "description": f"Version {deployment['version']} → {deployment['environment']}",
            "color": 0x00FF00,
            "fields": [
                {"name": "Duration", "value": f"{deployment['duration_seconds']}s"},
                {"name": "Strategy", "value": deployment['strategy']},
                {"name": "Success Rate", "value": f"{deployment['metrics']['success_rate']:.1f}%"}
            ]
        }
    elif event == 'deployment.failed':
        embed = {
            "title": f"❌ {deployment['fraise']} Deployment Failed",
            "description": deployment['error_message'],
            "color": 0xFF0000,
            "fields": [
                {"name": "Error", "value": deployment['error_code']},
                {"name": "Environment", "value": deployment['environment']},
                {"name": "Rollback", "value": "Yes" if deployment['rollback_performed'] else "No"}
            ]
        }

    # Send to Discord
    discord_url = os.getenv('DISCORD_WEBHOOK_URL')
    requests.post(discord_url, json={"embeds": [embed]})

    return 200, "OK"
```

### PagerDuty Integration

```python
@app.route('/webhook/pagerduty', methods=['POST'])
def pagerduty_webhook():
    data = request.json
    event = data['event']
    deployment = data['data']

    if event == 'deployment.failed':
        # Create PagerDuty incident
        incident_data = {
            "routing_key": os.getenv('PAGERDUTY_ROUTING_KEY'),
            "event_action": "trigger",
            "payload": {
                "summary": f"Deployment failed: {deployment['fraise']}/{deployment['environment']}",
                "severity": "critical",
                "source": "Fraisier",
                "custom_details": {
                    "deployment_id": deployment['deployment_id'],
                    "error": deployment['error_message'],
                    "version": deployment['version']
                }
            }
        }

        requests.post(
            "https://events.pagerduty.com/v2/enqueue",
            json=incident_data
        )

    elif event == 'deployment.completed' and deployment['rollback_performed']:
        # Resolve incident
        incident_data = {
            "routing_key": os.getenv('PAGERDUTY_ROUTING_KEY'),
            "event_action": "resolve",
            "dedup_key": f"deployment_{deployment['original_deployment_id']}"
        }

        requests.post(
            "https://events.pagerduty.com/v2/enqueue",
            json=incident_data
        )

    return 200, "OK"
```

### Custom Metrics Integration

```python
@app.route('/webhook/metrics', methods=['POST'])
def metrics_webhook():
    data = request.json
    deployment = data['data']

    if data['event'] == 'deployment.completed':
        # Push metrics to Prometheus pushgateway
        metrics = f"""
# HELP fraisier_deployment_duration_seconds Deployment duration in seconds
# TYPE fraisier_deployment_duration_seconds gauge
fraisier_deployment_duration_seconds{{fraise="{deployment['fraise']}", environment="{deployment['environment']}"}} {deployment['duration_seconds']}

# HELP fraisier_health_checks_passed Health checks passed
# TYPE fraisier_health_checks_passed gauge
fraisier_health_checks_passed{{fraise="{deployment['fraise']}", environment="{deployment['environment']}"}} {deployment['health_checks_passed']}

# HELP fraisier_success_rate Success rate percentage
# TYPE fraisier_success_rate gauge
fraisier_success_rate{{fraise="{deployment['fraise']}", environment="{deployment['environment']}"}} {deployment['metrics']['success_rate']}
"""

        requests.post(
            f"http://localhost:9091/metrics/job/fraisier",
            data=metrics,
            headers={"Content-Type": "text/plain"}
        )

    return 200, "OK"
```

---

## Webhook Management

### List Webhooks

```bash
fraisier webhook list [OPTIONS]
```

**Options**:

- `--event EVENT`: Filter by event type
- `--status STATUS`: Filter by status (active, failed, disabled)
- `--long / -l`: Detailed output

**Example**:

```bash
fraisier webhook list --event deployment.completed
```

**Output**:

```
Webhooks (4 total):

1. webhook_001 | deployment.completed | https://example.com/webhook
   │ Status: active
   │ Retries: 0
   │ Last: 2024-01-22 10:05:30
   │ Created: 2024-01-20 14:00:00

2. webhook_002 | deployment.* | https://slack.example.com/webhook
   │ Status: active
   │ Retries: 0
   │ Last: 2024-01-22 09:30:00
   │ Created: 2024-01-15 10:00:00
```

---

### Test Webhook

```bash
fraisier webhook test WEBHOOK_ID
```

Sends a test event to verify the webhook is working.

**Example**:

```bash
fraisier webhook test webhook_001

# Output:
# Testing webhook_001...
# POST https://example.com/webhook
# Status: 200 OK
# Response time: 125ms
```

---

### Update Webhook

```bash
fraisier webhook update WEBHOOK_ID [OPTIONS]
```

**Options**:

- `--active / --inactive`: Enable/disable
- `--event EVENT`: Update event subscriptions (repeatable)
- `--url URL`: Update URL
- `--retry-count N`: Update retry count

**Example**:

```bash
# Disable webhook
fraisier webhook update webhook_001 --inactive

# Add event subscription
fraisier webhook update webhook_001 --event deployment.failed --active

# Change URL
fraisier webhook update webhook_001 --url https://new.example.com/webhook
```

---

### Delete Webhook

```bash
fraisier webhook remove WEBHOOK_ID
```

**Example**:

```bash
fraisier webhook remove webhook_001
```

---

## Webhook Logs

View webhook delivery history:

```bash
fraisier webhook logs WEBHOOK_ID [OPTIONS]
```

**Options**:

- `--limit N`: Show last N deliveries (default: 50)
- `--status STATUS`: Filter by status (success, failed, pending)
- `--since TIME`: Show since this time (e.g., "1h", "1d")

**Example**:

```bash
# Recent deliveries
fraisier webhook logs webhook_001 --limit 20

# Failed deliveries
fraisier webhook logs webhook_001 --status failed --limit 100

# Last 24 hours
fraisier webhook logs webhook_001 --since 1d
```

---

## Best Practices

1. **Use Secrets**: Always set a secret and verify signatures
2. **Handle Retries**: Webhooks may be delivered multiple times; implement idempotency
3. **Use Event Filters**: Only subscribe to events you need
4. **Monitor Webhooks**: Check webhook logs regularly for failures
5. **Set Timeouts**: Implement request timeouts in your webhook handler
6. **Log Events**: Log all webhook events for debugging
7. **Test First**: Use `webhook test` before relying on webhooks
8. **Version Compatibility**: Keep webhook handlers backward compatible

---

## Troubleshooting

### Webhook Not Being Called

1. Check webhook is active: `fraisier webhook list`
2. Test webhook: `fraisier webhook test webhook_123`
3. Check logs: `fraisier webhook logs webhook_123 --status failed`
4. Verify event type: `fraisier webhook list webhook_123`

### Signature Verification Failing

1. Verify secret is correct: `export FRAISIER_WEBHOOK_SECRET=...`
2. Use raw request body for verification (not parsed JSON)
3. Check header format: `X-Fraisier-Signature: sha256=...`

### Webhook Deliveries Timing Out

1. Reduce webhook timeout: `fraisier webhook update webhook_123 --timeout 10`
2. Optimize webhook handler code
3. Check network connectivity
4. Increase retry delay: `fraisier webhook update webhook_123 --retry-delay 10`

---

## See Also

- [API_REFERENCE.md](API_REFERENCE.md) - REST API endpoints
- [CLI_REFERENCE.md](CLI_REFERENCE.md) - CLI commands
- [NATS_INTEGRATION_GUIDE.md](NATS_INTEGRATION_GUIDE.md) - Event bus integration
