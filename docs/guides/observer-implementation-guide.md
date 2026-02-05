<!-- Skip to main content -->
---
title: Observer Implementation Guide
description: Step-by-step implementation guide for setting up event-driven workflows using FraiseQL Observers and webhooks.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# Observer Implementation Guide

**Status:** ✅ Production Ready
**Audience:** Developers, Integration Engineers
**Reading Time:** 20-25 minutes
**Last Updated:** 2026-02-05

Step-by-step implementation guide for setting up event-driven workflows using FraiseQL Observers and webhooks.

---

## Overview

FraiseQL Observers react to database changes (mutations, CDC events) and trigger actions:

- Webhooks (Discord, Slack, GitHub, Stripe, etc.)
- Email notifications
- SMS alerts
- Search indexing updates
- External system integration

### Observer Flow Diagram

**Diagram:** System architecture visualization

```d2
<!-- Code example in D2 Diagram -->
direction: down

Mutation: "GraphQL Mutation\n(Create, Update, Delete)" {
  shape: box
  style.fill: "#e3f2fd"
}

Trigger: "Event Trigger\n(Observer checks event)" {
  shape: box
  style.fill: "#f3e5f5"
}

Filter: "Filter Match?\n(Optional conditions)" {
  shape: diamond
  style.fill: "#fff9c4"
}

FilterYes: "✅ Conditions met" {
  shape: box
  style.fill: "#f1f8e9"
}

FilterNo: "❌ Skip action" {
  shape: box
  style.fill: "#ffebee"
}

Action: "Execute Action\n(Webhook, Email, SMS)" {
  shape: box
  style.fill: "#fff3e0"
}

Webhook: "POST to external\nwebhook URL" {
  shape: box
  style.fill: "#ffe0b2"
}

Retry: "Retry on failure\n(exponential backoff)" {
  shape: box
  style.fill: "#ffccbc"
}

Success: "✅ Action complete\n(Log & continue)" {
  shape: box
  style.fill: "#c8e6c9"
}

Mutation -> Trigger
Trigger -> Filter
Filter -> FilterYes: "Yes"
Filter -> FilterNo: "No"
FilterYes -> Action
Action -> Webhook
Webhook -> Retry: "Failed"
Webhook -> Success: "Success"
Retry -> Webhook
```text
<!-- Code example in TEXT -->

---

## Quick Start: 5 Minute Setup

### Step 1: Define Observer

```python
<!-- Code example in Python -->
@FraiseQL.observer
class UserCreatedNotification:
    """Trigger when new user created."""
    trigger = Event.CREATE
    entity = "User"

    actions = [
        Webhook(
            provider="discord",
            url="https://discord.com/api/webhooks/YOUR_WEBHOOK_ID/YOUR_TOKEN",
            payload={
                "content": "New user: {user.name} ({user.email})"
            }
        )
    ]
```text
<!-- Code example in TEXT -->

### Step 2: Configure Webhook Provider

```toml
<!-- Code example in TOML -->
# FraiseQL.toml
[FraiseQL.webhooks.discord]
enabled = true
timeout_seconds = 30
retry_max_attempts = 3
retry_backoff_ms = 1000
```text
<!-- Code example in TEXT -->

### Step 3: Deploy and Test

```bash
<!-- Code example in BASH -->
# Compile
FraiseQL compile schema.py

# Start server
FraiseQL serve

# Create user and watch Discord notification appear
curl -X POST http://localhost:5000/graphql \
  -d '{"query": "mutation { createUser(name: \"Alice\", email: \"alice@example.com\") { id } }"}'
```text
<!-- Code example in TEXT -->

---

## Detailed Implementation

### Discord Webhooks

**Setup Discord Webhook:**

1. Create Discord server (if needed)
2. Go to Server Settings → Webhooks → New Webhook
3. Copy webhook URL

**FraiseQL Configuration:**

```python
<!-- Code example in Python -->
@FraiseQL.observer
class OrderNotification:
    trigger = Event.CREATE
    entity = "Order"
    filter = lambda event: event.data.get("status") == "completed"

    actions = [
        Webhook(
            provider="discord",
            url=os.getenv("DISCORD_WEBHOOK_URL"),
            payload={
                "embeds": [{
                    "title": "New Order",
                    "description": f"Order {event.data['id']} placed",
                    "fields": [
                        {"name": "Amount", "value": f"${event.data['total']}"},
                        {"name": "Customer", "value": event.data['customer_name']},
                    ],
                    "color": 5814783  # Blue
                }]
            }
        )
    ]
```text
<!-- Code example in TEXT -->

### Slack Webhooks

**Setup Slack Webhook:**

1. Go to [api.slack.com](https://api.slack.com/apps)
2. Create App → From scratch → Name + Workspace
3. Enable Incoming Webhooks
4. Create webhook for specific channel
5. Copy URL

**FraiseQL Configuration:**

```python
<!-- Code example in Python -->
@FraiseQL.observer
class AlertHighValue Order:
    trigger = Event.CREATE
    entity = "Order"
    filter = lambda e: e.data.get("total", 0) > 1000

    actions = [
        Webhook(
            provider="slack",
            url=os.getenv("SLACK_WEBHOOK_URL"),
            payload={
                "channel": "#sales-alerts",
                "text": ":moneybag: High-value order!",
                "blocks": [
                    {
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": f"*Order ID:* {event.data['id']}\n*Amount:* ${event.data['total']}"
                        }
                    }
                ]
            }
        )
    ]
```text
<!-- Code example in TEXT -->

### GitHub Integration

**Setup GitHub Webhook:**

1. Repository Settings → Webhooks → Add webhook
2. Payload URL: `http://your-app/webhooks/github`
3. Events: Select all relevant events
4. Content type: application/json
5. Create webhook

**FraiseQL Observer (Bidirectional):**

```python
<!-- Code example in Python -->
@FraiseQL.observer
class SyncIssueToDatabase:
    """When GitHub issue created, store in database."""
    trigger = Event.GITHUB_ISSUE_OPENED
    entity = "Issue"

    actions = [
        # Create database record
        Mutation(
            query="""
            mutation CreateIssue($title: str, $body: str, $github_id: str) {
                createIssue(title: $title, body: $body, github_id: $github_id) {
                    id
                }
            }
            """,
            variables={
                "title": event.data.get("issue", {}).get("title"),
                "body": event.data.get("issue", {}).get("body"),
                "github_id": str(event.data.get("issue", {}).get("id")),
            }
        )
    ]
```text
<!-- Code example in TEXT -->

### Email Notifications

**Setup SMTP:**

```toml
<!-- Code example in TOML -->
[FraiseQL.email]
provider = "smtp"
smtp_host = "smtp.gmail.com"
smtp_port = 587
smtp_username = "noreply@example.com"
smtp_password = "${SMTP_PASSWORD}"  # Use environment variable!
from_address = "noreply@example.com"
from_name = "FraiseQL App"
```text
<!-- Code example in TEXT -->

**FraiseQL Observer:**

```python
<!-- Code example in Python -->
@FraiseQL.observer
class SendWelcomeEmail:
    trigger = Event.CREATE
    entity = "User"

    actions = [
        Email(
            to=event.data.get("email"),
            subject="Welcome to our app!",
            template="welcome",
            variables={
                "user_name": event.data.get("name"),
                "activation_link": f"https://app.example.com/activate/{event.data.get('id')}"
            }
        )
    ]
```text
<!-- Code example in TEXT -->

### Search Indexing (Elasticsearch)

**Setup Elasticsearch:**

```bash
<!-- Code example in BASH -->
docker run -d -p 9200:9200 -e "xpack.security.enabled=false" docker.elastic.co/elasticsearch/elasticsearch:8.0.0
```text
<!-- Code example in TEXT -->

**FraiseQL Observer:**

```python
<!-- Code example in Python -->
@FraiseQL.observer
class IndexProductInSearch:
    trigger = Event.CREATE | Event.UPDATE
    entity = "Product"

    actions = [
        Http(
            url="http://elasticsearch:9200/products/_doc/{event.data['id']}",
            method="POST",
            payload={
                "name": event.data.get("name"),
                "description": event.data.get("description"),
                "price": event.data.get("price"),
                "tags": event.data.get("tags", []),
                "indexed_at": datetime.now().isoformat()
            }
        )
    ]
```text
<!-- Code example in TEXT -->

---

## Advanced Patterns

### Pattern 1: Conditional Actions

```python
<!-- Code example in Python -->
@FraiseQL.observer
class ConditionalNotification:
    trigger = Event.UPDATE
    entity = "Order"

    actions = [
        # Send Slack alert only if status changed to "shipped"
        Webhook(
            provider="slack",
            url=os.getenv("SLACK_WEBHOOK_URL"),
            condition=lambda e: e.old.get("status") != "shipped" and e.new.get("status") == "shipped",
            payload={"text": f"Order {e.data['id']} shipped!"}
        ),

        # Send email only if total > $500
        Email(
            to=event.data.get("customer_email"),
            condition=lambda e: e.data.get("total", 0) > 500,
            template="high_value_order"
        )
    ]
```text
<!-- Code example in TEXT -->

### Pattern 2: Chained Actions

```python
<!-- Code example in Python -->
@FraiseQL.observer
class OrderWorkflow:
    trigger = Event.CREATE
    entity = "Order"

    actions = [
        # 1. Save order event
        Mutation(query="mutation { logEvent(...) }"),

        # 2. Notify team
        Webhook(provider="slack", url="..."),

        # 3. Send customer confirmation
        Email(to=event.data.get("email"), template="order_confirmation"),

        # 4. Index for search
        Http(url="http://elasticsearch/...", method="POST")
    ]
```text
<!-- Code example in TEXT -->

### Pattern 3: Rate-Limited Actions

```python
<!-- Code example in Python -->
@FraiseQL.observer
class RateLimitedAlert:
    trigger = Event.CREATE
    entity = "ErrorLog"

    actions = [
        Webhook(
            provider="slack",
            url=os.getenv("SLACK_WEBHOOK_URL"),
            rate_limit=RateLimit(max_per_minute=5)  # Max 5 alerts/minute
        )
    ]
```text
<!-- Code example in TEXT -->

---

## Error Handling & Retry Logic

### Automatic Retries

```python
<!-- Code example in Python -->
@FraiseQL.observer
class ResilientWebhook:
    trigger = Event.CREATE
    entity = "Notification"

    actions = [
        Webhook(
            provider="custom",
            url="http://external-service/webhook",
            retry_policy=RetryPolicy(
                max_attempts=3,
                backoff_multiplier=2.0,  # 1s, 2s, 4s
                timeout_seconds=30
            )
        )
    ]
```text
<!-- Code example in TEXT -->

### Error Callbacks

```python
<!-- Code example in Python -->
@FraiseQL.observer
class HandleWebhookFailure:
    trigger = Event.CREATE
    entity = "Order"

    actions = [
        Webhook(
            provider="slack",
            url="...",
            on_failure=lambda error: log_to_database({
                "error_type": type(error).__name__,
                "error_message": str(error),
                "timestamp": datetime.now()
            })
        )
    ]
```text
<!-- Code example in TEXT -->

---

## Testing Observers

### Unit Test

```python
<!-- Code example in Python -->
import pytest
from unittest.mock import patch

@pytest.fixture
def mock_webhook():
    with patch('FraiseQL.webhooks.send') as mock:
        yield mock

def test_user_created_notification(mock_webhook):
    # Trigger user creation
    create_user("Alice", "alice@example.com")

    # Verify webhook called
    mock_webhook.assert_called_once()
    call_args = mock_webhook.call_args
    assert "Alice" in call_args.kwargs["payload"]["content"]
```text
<!-- Code example in TEXT -->

### Integration Test

```python
<!-- Code example in Python -->
@pytest.mark.integration
async def test_observer_end_to_end():
    # Start test server
    server = await start_test_server()

    # Mock webhook endpoint
    webhook_calls = []

    @app.post("/mock-webhook")
    async def mock_webhook(request):
        webhook_calls.append(await request.json())
        return {"status": "ok"}

    # Trigger mutation
    async with AsyncClient(url=server.url) as client:
        await client.query("mutation { createUser(...) }")

    # Verify webhook was called
    assert len(webhook_calls) == 1
    assert webhook_calls[0]["user"]["name"] == "Alice"
```text
<!-- Code example in TEXT -->

---

## Production Monitoring

### Monitor Observer Execution

```bash
<!-- Code example in BASH -->
# Check observer logs
tail -f /var/log/FraiseQL-observers.log | grep "OBSERVER"

# Expected output:
# [OBSERVER] OrderNotification triggered for Order#123
# [WEBHOOK] Sending to slack://...
# [WEBHOOK] Status: 200 OK
```text
<!-- Code example in TEXT -->

### Alert on Observer Failures

```toml
<!-- Code example in TOML -->
[FraiseQL.monitoring]
observer_failure_alert = true
observer_failure_threshold = 5  # Alert if 5+ failures in 5 minutes
```text
<!-- Code example in TEXT -->

### Metrics to Track

```prometheus
<!-- Code example in PROMETHEUS -->
# Observer metrics
fraiseql_observer_executions_total{observer="OrderNotification", status="success"}
fraiseql_observer_executions_total{observer="OrderNotification", status="failure"}
fraiseql_observer_latency_seconds{observer="OrderNotification", quantile="p95"}
fraiseql_webhook_deliveries_total{provider="slack", status="success"}
fraiseql_webhook_deliveries_total{provider="slack", status="failure"}
fraiseql_webhook_latency_seconds{provider="slack", quantile="p95"}
```text
<!-- Code example in TEXT -->

---

## See Also

**Related Guides:**

- **[Common Patterns](./patterns.md)** — Observer patterns in real applications
- **[Subscriptions Architecture](../architecture/realtime/subscriptions.md)** — Real-time alternatives to observers
- **[Integration Patterns](../integrations/README.md)** — Integration architecture

**Operations:**

- **[Monitoring & Observability](./monitoring.md)** — Observer monitoring in production
- **[Production Deployment](./production-deployment.md)** — Deploying observer workflows

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
