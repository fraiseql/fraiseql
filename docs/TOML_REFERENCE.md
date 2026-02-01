# FraiseQL TOML Configuration Reference

Complete reference for `fraiseql.toml` configuration in FraiseQL v2.

## Table of Contents

1. [Overview](#overview)
2. [Top-Level Configuration](#top-level-configuration)
3. [Queries](#queries)
4. [Mutations](#mutations)
5. [Subscriptions](#subscriptions)
6. [Security](#security)
7. [Federation](#federation)
8. [Observers](#observers)
9. [Analytics](#analytics)
10. [Caching](#caching)

---

## Overview

`fraiseql.toml` defines the operational configuration for your GraphQL API:
- Queries and mutations
- Security (rate limiting, authentication, audit logging)
- Federation (cross-service composition)
- Observers (event-driven actions on database changes)
- Analytics and observability
- Caching strategies

Types are provided by language SDKs via `types.json`, not defined in TOML.

---

## Top-Level Configuration

```toml
[fraiseql]
version = "2.0"                    # Required: FraiseQL version
name = "My API"                    # Optional: API name
description = "API description"   # Optional: Description
```

---

## Queries

Define GraphQL queries that resolve to your data.

### Basic Query

```toml
[fraiseql.queries.users]
return_type = "User"               # Required: Return type name
returns_list = true                # Required: Returns array?
nullable = false                   # Optional: Can be null? (default: false)
description = "Get all users"      # Optional: Query description
sql_source = "SELECT * FROM users" # Optional: SQL or view name
```

### Query with Arguments

```toml
[fraiseql.queries.user]
return_type = "User"
returns_list = false
nullable = true

[fraiseql.queries.user.arguments]
id = "String"                      # argument_name = "Type"
email = "String"
```

### Complete Query Example

```toml
[fraiseql.queries.users]
return_type = "User"
returns_list = true
nullable = false
description = "Paginated user list"
sql_source = "SELECT * FROM users LIMIT ? OFFSET ?"

[fraiseql.queries.users.arguments]
limit = "Int"
offset = "Int"
role = "String"    # Optional filter

[fraiseql.queries.users.cache]
ttl_seconds = 300
invalidate_on = ["User"]  # Invalidate when User changes
```

---

## Mutations

Define GraphQL mutations that modify your data.

### Basic Mutation

```toml
[fraiseql.mutations.createUser]
return_type = "User"
returns_list = false
nullable = false
operation = "CREATE"               # CREATE, UPDATE, DELETE, or CUSTOM
description = "Create a new user"
sql_source = "INSERT INTO users (...) VALUES (...) RETURNING *"
```

### Mutation with Arguments

```toml
[fraiseql.mutations.createUser.arguments]
name = "String"
email = "String"
role = "String"
```

### Complete Mutation Example

```toml
[fraiseql.mutations.updateUser]
return_type = "User"
returns_list = false
nullable = true
operation = "UPDATE"
description = "Update user information"
sql_source = "UPDATE users SET name = ?, email = ? WHERE id = ? RETURNING *"

[fraiseql.mutations.updateUser.arguments]
id = "String"
name = "String"
email = "String"
```

---

## Subscriptions

Define real-time subscriptions for database changes.

```toml
[fraiseql.subscriptions.userCreated]
entity_type = "User"               # Required: What type to subscribe to
returns_list = false
nullable = false
description = "Subscribe to new users"
topic = "users_created"            # Optional: Event topic/channel

[fraiseql.subscriptions.userCreated.arguments]
role_filter = "String"             # Optional: Filter events
```

---

## Security

### Rate Limiting

```toml
[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100      # Unauthenticated limit
auth_start_window_secs = 60
authenticated_max_requests = 1000  # Authenticated limit
authenticated_window_secs = 60
per_user_max_requests = 5000       # Per-user limit (optional)
per_user_window_secs = 3600
```

### Audit Logging

```toml
[fraiseql.security.audit_logging]
enabled = true
log_level = "info"                 # debug, info, warn, error
log_auth_attempts = true           # Log login attempts
log_mutations = true               # Log data changes
log_query_errors = true            # Log query failures
```

### Error Sanitization

```toml
[fraiseql.security.error_sanitization]
enabled = true
hide_internal_errors = true        # Hide DB errors from clients
sanitization_level = "strict"      # lenient, moderate, strict
```

### JWT/Auth

```toml
[fraiseql.security.jwt]
enabled = true
issuer = "https://auth.example.com"
audience = "https://api.example.com"
required = true                    # JWT required for all queries?

[[fraiseql.security.jwt.allowed_scopes]]
scope = "read:users"
operations = ["users", "user"]

[[fraiseql.security.jwt.allowed_scopes]]
scope = "write:users"
operations = ["createUser", "updateUser", "deleteUser"]
```

---

## Federation

Compose multiple GraphQL services into one.

### Basic Federation

```toml
[fraiseql.federation]
enabled = true
version = "v2"                     # Apollo Federation version

[[fraiseql.federation.subgraphs]]
name = "Users"
strategy = "local"                 # local = this database
table_name = "users"

[[fraiseql.federation.subgraphs]]
name = "Orders"
strategy = "http"                  # http = external service
url = "http://orders-service/graphql"
timeout_ms = 5000
```

### HTTP Subgraph with Auth

```toml
[[fraiseql.federation.subgraphs]]
name = "Payments"
strategy = "http"
url = "https://payments-service/graphql"
timeout_ms = 10000
max_retries = 3
retry_delay_ms = 100

[fraiseql.federation.subgraphs.auth]
type = "bearer"                    # bearer, basic, or custom
token_env = "PAYMENTS_API_TOKEN"
```

---

## Observers

Event-driven actions triggered by database changes.

### Basic Observer

```toml
[fraiseql.observers.userCreated]
entity = "User"                    # Required: What entity to watch
event = "INSERT"                   # Required: INSERT, UPDATE, DELETE
description = "Welcome new users"
```

### Observer with Condition

```toml
[fraiseql.observers.highValueOrder]
entity = "Order"
event = "INSERT"
condition = "total > 1000"         # Optional: Trigger only if true
description = "Alert on high-value orders"
```

### Observer with Webhook Action

```toml
[[fraiseql.observers.userCreated.actions]]
type = "webhook"
url = "https://api.example.com/webhooks/user-created"
method = "POST"                    # GET, POST, PUT, DELETE
timeout_ms = 5000
headers = { "Authorization" = "Bearer ${WEBHOOK_TOKEN}" }
body_template = "{...}"            # Optional JSON template
```

### Observer with Slack Action

```toml
[[fraiseql.observers.orderShipped.actions]]
type = "slack"
channel = "#orders"
webhook_url = "${SLACK_WEBHOOK_URL}"
message = "Order {id} shipped to {customer}"
```

### Observer with Email Action

```toml
[[fraiseql.observers.userCreated.actions]]
type = "email"
to = "{user_email}"
from = "noreply@example.com"
subject = "Welcome to {app_name}"
body = "Thanks for joining!"
```

### Observer Retry Policy

```toml
[fraiseql.observers.criticalWebhook]
entity = "Payment"
event = "UPDATE"

[[fraiseql.observers.criticalWebhook.actions]]
type = "webhook"
url = "https://payment-processor/confirm"

[fraiseql.observers.criticalWebhook.retry]
max_attempts = 5                   # Retry how many times?
initial_delay_ms = 100             # Start with 100ms delay
max_delay_ms = 60000               # Cap at 60 seconds
multiplier = 2.0                   # Exponential backoff: 100 → 200 → 400 → ...
```

---

## Analytics

Schema analytics and observability configuration.

```toml
[fraiseql.analytics]
enabled = true

[fraiseql.analytics.metrics]
collect_query_performance = true   # Track query execution time
collect_field_usage = true         # Track which fields are used
collect_error_rates = true         # Track errors
collect_cache_hits = true          # Track cache effectiveness

[fraiseql.analytics.export]
type = "prometheus"                # prometheus, datadog, cloudwatch
endpoint = "http://prometheus:9090"
```

---

## Caching

Query result caching with automatic coherency management.

### Basic Caching

```toml
[fraiseql.caching]
enabled = true
default_ttl_seconds = 300          # Default: 5 minutes

[[fraiseql.caching.rules]]
query = "users"                    # Query name to cache
ttl_seconds = 600                  # Cache for 10 minutes
invalidate_on = ["User"]           # Clear when User changes
```

### Cache Invalidation

```toml
[[fraiseql.caching.rules]]
query = "posts"
ttl_seconds = 300
invalidate_on = ["Post"]           # Single invalidator

[[fraiseql.caching.rules]]
query = "userWithPosts"
ttl_seconds = 600
invalidate_on = ["User", "Post"]   # Multiple invalidators
```

### Cache Key Customization

```toml
[[fraiseql.caching.rules]]
query = "userByEmail"
ttl_seconds = 300
cache_key_args = ["email"]         # Only vary by email argument
invalidate_on = ["User"]
```

---

## Environment Variables

Reference environment variables in TOML using `${VAR_NAME}`:

```toml
[fraiseql.federation.subgraphs.payments]
url = "${PAYMENTS_SERVICE_URL}"
timeout_ms = 5000

[fraiseql.security.jwt]
issuer = "${AUTH_ISSUER_URL}"
audience = "${API_AUDIENCE}"

[[fraiseql.observers.webhook.actions]]
url = "${WEBHOOK_URL}"
headers = { "Authorization" = "Bearer ${WEBHOOK_TOKEN}" }
```

---

## Complete Example

```toml
[fraiseql]
version = "2.0"
name = "E-commerce API"
description = "Complete example with all features"

# ============================================================================
# QUERIES
# ============================================================================

[fraiseql.queries.users]
return_type = "User"
returns_list = true
description = "Get all users with pagination"
sql_source = "SELECT * FROM users LIMIT ? OFFSET ?"

[fraiseql.queries.users.arguments]
limit = "Int"
offset = "Int"

# ============================================================================
# MUTATIONS
# ============================================================================

[fraiseql.mutations.createUser]
return_type = "User"
returns_list = false
operation = "CREATE"
sql_source = "INSERT INTO users (name, email) VALUES (?, ?) RETURNING *"

[fraiseql.mutations.createUser.arguments]
name = "String"
email = "String"

# ============================================================================
# SECURITY
# ============================================================================

[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60

[fraiseql.security.audit_logging]
enabled = true
log_level = "info"
log_mutations = true

# ============================================================================
# OBSERVERS
# ============================================================================

[fraiseql.observers.userCreated]
entity = "User"
event = "INSERT"
description = "Send welcome email on user creation"

[[fraiseql.observers.userCreated.actions]]
type = "email"
to = "{email}"
from = "welcome@example.com"
subject = "Welcome!"
body = "Thanks for signing up"

# ============================================================================
# CACHING
# ============================================================================

[fraiseql.caching]
enabled = true
default_ttl_seconds = 300

[[fraiseql.caching.rules]]
query = "users"
ttl_seconds = 600
invalidate_on = ["User"]
```

---

## Best Practices

1. **Use meaningful names** - Query names should describe what they do
2. **Document your queries** - Add descriptions for API consumers
3. **Set appropriate cache TTLs** - Balance freshness vs performance
4. **Configure rate limiting** - Protect your API from abuse
5. **Use environment variables** - Don't hardcode secrets
6. **Test observers** - Ensure webhooks are reachable
7. **Monitor performance** - Use analytics to identify bottlenecks

---

For more information:
- [Migration Guide](./MIGRATION_GUIDE.md)
- [API Documentation](./API.md)
- [Examples](../tests/integration/examples/)
