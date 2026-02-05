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

---

## Quick Start: 5 Minute Setup

### Step 1: Define Observer

```python
@fraiseql.observer
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
```

### Step 2: Configure Webhook Provider

```toml
# fraiseql.toml
[fraiseql.webhooks.discord]
enabled = true
timeout_seconds = 30
retry_max_attempts = 3
retry_backoff_ms = 1000
```

### Step 3: Deploy and Test

```bash
# Compile
fraiseql compile schema.py

# Start server
fraiseql serve

# Create user and watch Discord notification appear
curl -X POST http://localhost:5000/graphql \
  -d '{"query": "mutation { createUser(name: \"Alice\", email: \"alice@example.com\") { id } }"}'
```

---

## Detailed Implementation

### Discord Webhooks

**Setup Discord Webhook:**
1. Create Discord server (if needed)
2. Go to Server Settings → Webhooks → New Webhook
3. Copy webhook URL

**FraiseQL Configuration:**

```python
@fraiseql.observer
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
```

### Slack Webhooks

**Setup Slack Webhook:**
1. Go to [api.slack.com](https://api.slack.com/apps)
2. Create App → From scratch → Name + Workspace
3. Enable Incoming Webhooks
4. Create webhook for specific channel
5. Copy URL

**FraiseQL Configuration:**

```python
@fraiseql.observer
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
```

### GitHub Integration

**Setup GitHub Webhook:**
1. Repository Settings → Webhooks → Add webhook
2. Payload URL: `http://your-app/webhooks/github`
3. Events: Select all relevant events
4. Content type: application/json
5. Create webhook

**FraiseQL Observer (Bidirectional):**

```python
@fraiseql.observer
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
```

### Email Notifications

**Setup SMTP:**

```toml
[fraiseql.email]
provider = "smtp"
smtp_host = "smtp.gmail.com"
smtp_port = 587
smtp_username = "noreply@example.com"
smtp_password = "${SMTP_PASSWORD}"  # Use environment variable!
from_address = "noreply@example.com"
from_name = "FraiseQL App"
```

**FraiseQL Observer:**

```python
@fraiseql.observer
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
```

### Search Indexing (Elasticsearch)

**Setup Elasticsearch:**

```bash
docker run -d -p 9200:9200 -e "xpack.security.enabled=false" docker.elastic.co/elasticsearch/elasticsearch:8.0.0
```

**FraiseQL Observer:**

```python
@fraiseql.observer
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
```

---

## Advanced Patterns

### Pattern 1: Conditional Actions

```python
@fraiseql.observer
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
```

### Pattern 2: Chained Actions

```python
@fraiseql.observer
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
```

### Pattern 3: Rate-Limited Actions

```python
@fraiseql.observer
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
```

---

## Error Handling & Retry Logic

### Automatic Retries

```python
@fraiseql.observer
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
```

### Error Callbacks

```python
@fraiseql.observer
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
```

---

## Testing Observers

### Unit Test

```python
import pytest
from unittest.mock import patch

@pytest.fixture
def mock_webhook():
    with patch('fraiseql.webhooks.send') as mock:
        yield mock

def test_user_created_notification(mock_webhook):
    # Trigger user creation
    create_user("Alice", "alice@example.com")

    # Verify webhook called
    mock_webhook.assert_called_once()
    call_args = mock_webhook.call_args
    assert "Alice" in call_args.kwargs["payload"]["content"]
```

### Integration Test

```python
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
```

---

## Production Monitoring

### Monitor Observer Execution

```bash
# Check observer logs
tail -f /var/log/fraiseql-observers.log | grep "OBSERVER"

# Expected output:
# [OBSERVER] OrderNotification triggered for Order#123
# [WEBHOOK] Sending to slack://...
# [WEBHOOK] Status: 200 OK
```

### Alert on Observer Failures

```toml
[fraiseql.monitoring]
observer_failure_alert = true
observer_failure_threshold = 5  # Alert if 5+ failures in 5 minutes
```

### Metrics to Track

```prometheus
# Observer metrics
fraiseql_observer_executions_total{observer="OrderNotification", status="success"}
fraiseql_observer_executions_total{observer="OrderNotification", status="failure"}
fraiseql_observer_latency_seconds{observer="OrderNotification", quantile="p95"}
fraiseql_webhook_deliveries_total{provider="slack", status="success"}
fraiseql_webhook_deliveries_total{provider="slack", status="failure"}
fraiseql_webhook_latency_seconds{provider="slack", quantile="p95"}
```

---

## See Also

**Related Guides:**
- **[Common Patterns](./PATTERNS.md)** — Observer patterns in real applications
- **[Subscriptions Architecture](../architecture/realtime/subscriptions.md)** — Real-time alternatives to observers
- **[Change Data Capture](../integrations/cdc/README.md)** — CDC event source for observers

**Integration Guides:**
- **[Webhook Providers](../integrations/webhooks/README.md)** — Complete webhook provider reference
- **[Integration Patterns](../integrations/README.md)** — Integration architecture

**Operations:**
- **[Monitoring & Observability](./monitoring.md)** — Observer monitoring in production
- **[Production Deployment](./production-deployment.md)** — Deploying observer workflows

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
