# Sentry Error Tracking

Enterprise-grade error tracking and performance monitoring for FraiseQL applications using Sentry.

## Overview

Sentry provides:
- **Automatic error capture** - Exceptions captured with full stack traces
- **Performance monitoring** - Track slow GraphQL queries and database calls
- **Release tracking** - Group errors by deployment version
- **Context capture** - User info, GraphQL queries, custom data

## Quick Start

### 1. Install Sentry SDK

```bash
pip install sentry-sdk[fastapi]
```

### 2. Initialize in Your Application

```python
from fraiseql.monitoring import init_sentry
import os

# Initialize Sentry
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    environment=os.getenv("ENVIRONMENT", "production"),
    traces_sample_rate=0.1,  # 10% of transactions
    profiles_sample_rate=0.1,  # 10% profiling
    release="fraiseql@0.11.0"
)
```

### 3. Get Your Sentry DSN

1. Create account at [sentry.io](https://sentry.io)
2. Create a new project → Select "FastAPI"
3. Copy the DSN: `https://xxxxx@sentry.io/xxxxx`
4. Add to environment: `export SENTRY_DSN="https://..."`

## Configuration

### Basic Configuration

```python
from fraiseql.monitoring import init_sentry

# Minimal setup
init_sentry(dsn=os.getenv("SENTRY_DSN"))

# Production setup
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    environment="production",
    traces_sample_rate=0.1,  # Sample 10% of transactions
    profiles_sample_rate=0.1,  # Profile 10% of requests
    release="fraiseql@0.11.0",
    server_name="api-server-01"
)
```

### Environment-Specific Configuration

```python
# Development - high sampling, all errors
if os.getenv("ENVIRONMENT") == "development":
    init_sentry(
        dsn=os.getenv("SENTRY_DSN"),
        environment="development",
        traces_sample_rate=1.0,  # 100% tracing
        send_default_pii=True
    )

# Production - conservative sampling
else:
    init_sentry(
        dsn=os.getenv("SENTRY_DSN"),
        environment="production",
        traces_sample_rate=0.1,  # 10% tracing
        send_default_pii=False  # Don't send PII
    )
```

## Manual Error Capture

### Capture Exceptions

```python
from fraiseql.monitoring import capture_exception

try:
    result = await risky_operation()
except Exception as e:
    # Capture with context
    event_id = capture_exception(
        e,
        level="error",
        extra={
            "user_id": user.id,
            "query": graphql_query,
            "variables": graphql_variables
        }
    )
    logger.error(f"Operation failed, Sentry event: {event_id}")
    raise
```

### Capture Messages

```python
from fraiseql.monitoring import capture_message

# Info message
capture_message(
    "User performed expensive operation",
    level="info",
    extra={"query_complexity": 1500}
)

# Warning message
capture_message(
    "Rate limit approaching",
    level="warning",
    extra={"current_rate": 95, "limit": 100}
)
```

## Context and User Tracking

### Set User Context

```python
from fraiseql.monitoring import set_user

@fraiseql.query
async def current_user(info) -> User:
    user = await get_authenticated_user(info)

    # Set user for error tracking
    set_user(
        user_id=user.id,
        email=user.email,
        username=user.username,
        subscription_tier=user.subscription_tier
    )

    return user
```

### Set Custom Context

```python
from fraiseql.monitoring import set_context

@fraiseql.query
async def search_products(info, query: str) -> list[Product]:
    # Add GraphQL query context
    set_context("graphql", {
        "operation": "search_products",
        "query": query,
        "complexity": calculate_complexity(info)
    })

    # Add business context
    set_context("search", {
        "term": query,
        "filters": info.variable_values.get("filters"),
        "result_count": 0  # Will be updated
    })

    results = await search(query)

    # Update context
    set_context("search", {"result_count": len(results)})

    return results
```

## GraphQL Integration

### Mutation Error Handling

```python
from fraiseql.monitoring import capture_exception, set_context

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> CreateUserResult:
    # Set context for this operation
    set_context("mutation", {
        "operation": "create_user",
        "input": input.dict()
    })

    try:
        user = await repo.create("user", input.dict())
        return CreateUserSuccess(user=user)

    except ValidationError as e:
        # Don't capture validation errors
        return CreateUserError(
            message="Invalid input",
            code="VALIDATION_ERROR"
        )

    except Exception as e:
        # Capture unexpected errors
        event_id = capture_exception(e, level="error")
        logger.error(f"User creation failed: {event_id}")

        return CreateUserError(
            message="Internal server error",
            code="INTERNAL_ERROR"
        )
```

### Query Performance Tracking

Sentry automatically tracks slow GraphQL queries with the FastAPI integration.

**Customize transaction names:**

```python
from fraiseql.monitoring import set_context
import sentry_sdk

@fraiseql.query
async def expensive_report(info) -> Report:
    # Set custom transaction name
    with sentry_sdk.start_transaction(
        op="graphql.query",
        name="expensive_report"
    ) as transaction:

        # Add spans for sub-operations
        with transaction.start_child(
            op="db.query",
            description="Load report data"
        ):
            data = await load_report_data()

        with transaction.start_child(
            op="compute",
            description="Calculate aggregates"
        ):
            aggregates = calculate_aggregates(data)

        return Report(data=data, aggregates=aggregates)
```

## Kubernetes Deployment

### Using Environment Variables

```yaml
# deployment.yaml
env:
  - name: SENTRY_DSN
    valueFrom:
      secretKeyRef:
        name: fraiseql-secrets
        key: SENTRY_DSN
  - name: SENTRY_ENVIRONMENT
    value: "production"
  - name: SENTRY_RELEASE
    value: "fraiseql@0.11.0"
```

### Using Helm Chart

```yaml
# values.yaml
sentry:
  enabled: true
  environment: "production"
  traceSampleRate: 0.1

secrets:
  existingSecret: "fraiseql-secrets"
```

## Release Tracking

### Automated Releases

```python
import os
from fraiseql.monitoring import init_sentry

# Get version from environment or package
version = os.getenv("RELEASE_VERSION", "0.11.0")

init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    release=f"fraiseql@{version}",
    environment=os.getenv("ENVIRONMENT", "production")
)
```

### Create Release in Sentry

```bash
# Using Sentry CLI
sentry-cli releases new "fraiseql@0.11.0"
sentry-cli releases set-commits "fraiseql@0.11.0" --auto
sentry-cli releases finalize "fraiseql@0.11.0"
sentry-cli releases deploys "fraiseql@0.11.0" new -e production
```

## Performance Monitoring

### Transaction Sampling

```python
# production.py
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    traces_sample_rate=0.1,  # 10% of all transactions

    # Or use custom sampling
    traces_sampler=lambda sampling_context: {
        "graphql.query": 0.05,    # 5% of queries
        "graphql.mutation": 0.5,  # 50% of mutations
        "default": 0.1            # 10% of others
    }.get(sampling_context["transaction_context"]["op"], 0.1)
)
```

### Profiling

```python
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    traces_sample_rate=0.1,
    profiles_sample_rate=0.1,  # Profile 10% of transactions

    # Python profiler integration
    enable_profiling=True
)
```

## Filtering Sensitive Data

### Scrub PII

```python
from sentry_sdk.scrubber import EventScrubber

init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    event_scrubber=EventScrubber(
        # Scrub these keys
        denylist=["password", "api_key", "token", "secret", "credit_card"]
    )
)
```

### Before Send Hook

```python
def before_send(event, hint):
    # Remove sensitive query parameters
    if "request" in event:
        if "query_string" in event["request"]:
            event["request"]["query_string"] = "[Filtered]"

    # Remove sensitive headers
    if "headers" in event.get("request", {}):
        sensitive_headers = ["authorization", "cookie"]
        for header in sensitive_headers:
            if header in event["request"]["headers"]:
                event["request"]["headers"][header] = "[Filtered]"

    return event

init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    before_send=before_send
)
```

## Best Practices

### 1. Use Structured Logging

```python
import structlog

logger = structlog.get_logger()

try:
    result = await operation()
except Exception as e:
    logger.error(
        "operation_failed",
        error=str(e),
        user_id=user.id,
        operation="create_order",
        exc_info=True
    )
    capture_exception(e)
    raise
```

### 2. Add Contextual Information

```python
# At request start
set_user(user_id=user.id, email=user.email)
set_context("request", {
    "endpoint": "/graphql",
    "method": "POST",
    "ip": request.client.host
})

# In mutations
set_context("mutation", {
    "operation": info.field_name,
    "input_size": len(str(input))
})
```

### 3. Group Similar Errors

```python
from sentry_sdk import configure_scope

with configure_scope() as scope:
    # Fingerprint for grouping
    scope.fingerprint = ["database-connection", db_host]
    capture_exception(db_error)
```

### 4. Set Appropriate Sample Rates

```yaml
# Development - capture everything
development:
  traces_sample_rate: 1.0
  profiles_sample_rate: 1.0

# Staging - high sampling
staging:
  traces_sample_rate: 0.5
  profiles_sample_rate: 0.5

# Production - conservative
production:
  traces_sample_rate: 0.1
  profiles_sample_rate: 0.1
```

## Troubleshooting

### Verify Sentry is Working

```python
from fraiseql.monitoring import capture_message

# Send test event
capture_message("Sentry integration test", level="info")
```

### Check Sentry Status

```python
import sentry_sdk

# Get current client
client = sentry_sdk.Hub.current.client

if client:
    print(f"Sentry enabled: {client.dsn}")
else:
    print("Sentry not initialized")
```

### Debug Mode

```python
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    debug=True,  # Print diagnostic information
    environment="development"
)
```

## Resources

- [Sentry Documentation](https://docs.sentry.io/platforms/python/)
- [FastAPI Integration](https://docs.sentry.io/platforms/python/integrations/fastapi/)
- [Performance Monitoring](https://docs.sentry.io/product/performance/)
- [Release Tracking](https://docs.sentry.io/product/releases/)
