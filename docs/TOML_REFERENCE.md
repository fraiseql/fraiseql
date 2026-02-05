<!-- Skip to main content -->
---
title: FraiseQL TOML Configuration Reference
description: Complete reference for `FraiseQL.toml` configuration in FraiseQL v2.
keywords: ["directives", "types", "scalars", "schema", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL TOML Configuration Reference

Complete reference for `FraiseQL.toml` configuration in FraiseQL v2.

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

`FraiseQL.toml` defines the operational configuration for your GraphQL API:

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
<!-- Code example in TOML -->
[schema]
version = "2.0"                      # Required: Schema version
name = "My API"                      # Required: API name
description = "API description"      # Optional: Description
database_target = "postgresql"       # Required: Database type (postgresql, mysql, sqlite, sqlserver)

[database]
url = "postgresql://localhost/mydb"  # Required: Database connection URL
pool_size = 10                       # Optional: Connection pool size (default: 10)
ssl_mode = "prefer"                  # Optional: SSL mode (disable, allow, prefer, require)
timeout_seconds = 30                 # Optional: Connection timeout (default: 30)
```text
<!-- Code example in TEXT -->

---

## Queries

Define GraphQL queries that resolve to your data.

### Basic Query

```toml
<!-- Code example in TOML -->
[FraiseQL.queries.users]
return_type = "User"               # Required: Return type name
returns_list = true                # Required: Returns array?
nullable = false                   # Optional: Can be null? (default: false)
description = "Get all users"      # Optional: Query description
sql_source = "SELECT * FROM users" # Optional: SQL or view name
```text
<!-- Code example in TEXT -->

### Query with Arguments

```toml
<!-- Code example in TOML -->
[FraiseQL.queries.user]
return_type = "User"
returns_list = false
nullable = true

[FraiseQL.queries.user.arguments]
id = "String"                      # argument_name = "Type"
email = "String"
```text
<!-- Code example in TEXT -->

### Complete Query Example

```toml
<!-- Code example in TOML -->
[FraiseQL.queries.users]
return_type = "User"
returns_list = true
nullable = false
description = "Paginated user list"
sql_source = "SELECT * FROM users LIMIT ? OFFSET ?"

[FraiseQL.queries.users.arguments]
limit = "Int"
offset = "Int"
role = "String"    # Optional filter

[FraiseQL.queries.users.cache]
ttl_seconds = 300
invalidate_on = ["User"]  # Invalidate when User changes
```text
<!-- Code example in TEXT -->

---

## Mutations

Define GraphQL mutations that modify your data.

### Basic Mutation

```toml
<!-- Code example in TOML -->
[FraiseQL.mutations.createUser]
return_type = "User"
returns_list = false
nullable = false
operation = "CREATE"               # CREATE, UPDATE, DELETE, or CUSTOM
description = "Create a new user"
sql_source = "INSERT INTO users (...) VALUES (...) RETURNING *"
```text
<!-- Code example in TEXT -->

### Mutation with Arguments

```toml
<!-- Code example in TOML -->
[FraiseQL.mutations.createUser.arguments]
name = "String"
email = "String"
role = "String"
```text
<!-- Code example in TEXT -->

### Complete Mutation Example

```toml
<!-- Code example in TOML -->
[FraiseQL.mutations.updateUser]
return_type = "User"
returns_list = false
nullable = true
operation = "UPDATE"
description = "Update user information"
sql_source = "UPDATE users SET name = ?, email = ? WHERE id = ? RETURNING *"

[FraiseQL.mutations.updateUser.arguments]
id = "String"
name = "String"
email = "String"
```text
<!-- Code example in TEXT -->

---

## Subscriptions

Define real-time subscriptions for database changes.

```toml
<!-- Code example in TOML -->
[FraiseQL.subscriptions.userCreated]
entity_type = "User"               # Required: What type to subscribe to
returns_list = false
nullable = false
description = "Subscribe to new users"
topic = "users_created"            # Optional: Event topic/channel

[FraiseQL.subscriptions.userCreated.arguments]
role_filter = "String"             # Optional: Filter events
```text
<!-- Code example in TEXT -->

---

## Security

### Rate Limiting

```toml
<!-- Code example in TOML -->
[FraiseQL.security.rate_limiting]
enabled = true
auth_start_max_requests = 100      # Unauthenticated limit
auth_start_window_secs = 60
authenticated_max_requests = 1000  # Authenticated limit
authenticated_window_secs = 60
per_user_max_requests = 5000       # Per-user limit (optional)
per_user_window_secs = 3600
```text
<!-- Code example in TEXT -->

### Audit Logging

```toml
<!-- Code example in TOML -->
[FraiseQL.security.audit_logging]
enabled = true
log_level = "info"                 # debug, info, warn, error
log_auth_attempts = true           # Log login attempts
log_mutations = true               # Log data changes
log_query_errors = true            # Log query failures
```text
<!-- Code example in TEXT -->

### Error Sanitization

```toml
<!-- Code example in TOML -->
[FraiseQL.security.error_sanitization]
enabled = true
hide_internal_errors = true        # Hide DB errors from clients
sanitization_level = "strict"      # lenient, moderate, strict
```text
<!-- Code example in TEXT -->

### JWT/Auth

```toml
<!-- Code example in TOML -->
[FraiseQL.security.jwt]
enabled = true
issuer = "https://auth.example.com"
audience = "https://api.example.com"
required = true                    # JWT required for all queries?

[[FraiseQL.security.jwt.allowed_scopes]]
scope = "read:users"
operations = ["users", "user"]

[[FraiseQL.security.jwt.allowed_scopes]]
scope = "write:users"
operations = ["createUser", "updateUser", "deleteUser"]
```text
<!-- Code example in TEXT -->

---

## Federation

Compose multiple GraphQL services into one.

### Basic Federation

```toml
<!-- Code example in TOML -->
[FraiseQL.federation]
enabled = true
version = "v2"                     # Apollo Federation version

[[FraiseQL.federation.subgraphs]]
name = "Users"
strategy = "local"                 # local = this database
table_name = "users"

[[FraiseQL.federation.subgraphs]]
name = "Orders"
strategy = "http"                  # http = external service
url = "http://orders-service/graphql"
timeout_ms = 5000
```text
<!-- Code example in TEXT -->

### HTTP Subgraph with Auth

```toml
<!-- Code example in TOML -->
[[FraiseQL.federation.subgraphs]]
name = "Payments"
strategy = "http"
url = "https://payments-service/graphql"
timeout_ms = 10000
max_retries = 3
retry_delay_ms = 100

[FraiseQL.federation.subgraphs.auth]
type = "bearer"                    # bearer, basic, or custom
token_env = "PAYMENTS_API_TOKEN"
```text
<!-- Code example in TEXT -->

---

## Observers

Event-driven actions triggered by database changes.

### Basic Observer

```toml
<!-- Code example in TOML -->
[FraiseQL.observers.userCreated]
entity = "User"                    # Required: What entity to watch
event = "INSERT"                   # Required: INSERT, UPDATE, DELETE
description = "Welcome new users"
```text
<!-- Code example in TEXT -->

### Observer with Condition

```toml
<!-- Code example in TOML -->
[FraiseQL.observers.highValueOrder]
entity = "Order"
event = "INSERT"
condition = "total > 1000"         # Optional: Trigger only if true
description = "Alert on high-value orders"
```text
<!-- Code example in TEXT -->

### Observer with Webhook Action

```toml
<!-- Code example in TOML -->
[[FraiseQL.observers.userCreated.actions]]
type = "webhook"
url = "https://api.example.com/webhooks/user-created"
method = "POST"                    # GET, POST, PUT, DELETE
timeout_ms = 5000
headers = { "Authorization" = "Bearer ${WEBHOOK_TOKEN}" }
body_template = "{...}"            # Optional JSON template
```text
<!-- Code example in TEXT -->

### Observer with Slack Action

```toml
<!-- Code example in TOML -->
[[FraiseQL.observers.orderShipped.actions]]
type = "slack"
channel = "#orders"
webhook_url = "${SLACK_WEBHOOK_URL}"
message = "Order {id} shipped to {customer}"
```text
<!-- Code example in TEXT -->

### Observer with Email Action

```toml
<!-- Code example in TOML -->
[[FraiseQL.observers.userCreated.actions]]
type = "email"
to = "{user_email}"
from = "noreply@example.com"
subject = "Welcome to {app_name}"
body = "Thanks for joining!"
```text
<!-- Code example in TEXT -->

### Observer Retry Policy

```toml
<!-- Code example in TOML -->
[FraiseQL.observers.criticalWebhook]
entity = "Payment"
event = "UPDATE"

[[FraiseQL.observers.criticalWebhook.actions]]
type = "webhook"
url = "https://payment-processor/confirm"

[FraiseQL.observers.criticalWebhook.retry]
max_attempts = 5                   # Retry how many times?
initial_delay_ms = 100             # Start with 100ms delay
max_delay_ms = 60000               # Cap at 60 seconds
multiplier = 2.0                   # Exponential backoff: 100 → 200 → 400 → ...
```text
<!-- Code example in TEXT -->

---

## Analytics

Schema analytics and observability configuration.

```toml
<!-- Code example in TOML -->
[FraiseQL.analytics]
enabled = true

[FraiseQL.analytics.metrics]
collect_query_performance = true   # Track query execution time
collect_field_usage = true         # Track which fields are used
collect_error_rates = true         # Track errors
collect_cache_hits = true          # Track cache effectiveness

[FraiseQL.analytics.export]
type = "prometheus"                # prometheus, datadog, cloudwatch
endpoint = "http://prometheus:9090"
```text
<!-- Code example in TEXT -->

---

## Caching

Query result caching with automatic coherency management.

### Basic Caching

```toml
<!-- Code example in TOML -->
[FraiseQL.caching]
enabled = true
default_ttl_seconds = 300          # Default: 5 minutes

[[FraiseQL.caching.rules]]
query = "users"                    # Query name to cache
ttl_seconds = 600                  # Cache for 10 minutes
invalidate_on = ["User"]           # Clear when User changes
```text
<!-- Code example in TEXT -->

### Cache Invalidation

```toml
<!-- Code example in TOML -->
[[FraiseQL.caching.rules]]
query = "posts"
ttl_seconds = 300
invalidate_on = ["Post"]           # Single invalidator

[[FraiseQL.caching.rules]]
query = "userWithPosts"
ttl_seconds = 600
invalidate_on = ["User", "Post"]   # Multiple invalidators
```text
<!-- Code example in TEXT -->

### Cache Key Customization

```toml
<!-- Code example in TOML -->
[[FraiseQL.caching.rules]]
query = "userByEmail"
ttl_seconds = 300
cache_key_args = ["email"]         # Only vary by email argument
invalidate_on = ["User"]
```text
<!-- Code example in TEXT -->

---

## Environment Variables

Reference environment variables in TOML using `${VAR_NAME}`:

```toml
<!-- Code example in TOML -->
[FraiseQL.federation.subgraphs.payments]
url = "${PAYMENTS_SERVICE_URL}"
timeout_ms = 5000

[FraiseQL.security.jwt]
issuer = "${AUTH_ISSUER_URL}"
audience = "${API_AUDIENCE}"

[[FraiseQL.observers.webhook.actions]]
url = "${WEBHOOK_URL}"
headers = { "Authorization" = "Bearer ${WEBHOOK_TOKEN}" }
```text
<!-- Code example in TEXT -->

---

## Complete Example

```toml
<!-- Code example in TOML -->
[FraiseQL]
version = "2.0"
name = "E-commerce API"
description = "Complete example with all features"

# ============================================================================
# QUERIES
# ============================================================================

[FraiseQL.queries.users]
return_type = "User"
returns_list = true
description = "Get all users with pagination"
sql_source = "SELECT * FROM users LIMIT ? OFFSET ?"

[FraiseQL.queries.users.arguments]
limit = "Int"
offset = "Int"

# ============================================================================
# MUTATIONS
# ============================================================================

[FraiseQL.mutations.createUser]
return_type = "User"
returns_list = false
operation = "CREATE"
sql_source = "INSERT INTO users (name, email) VALUES (?, ?) RETURNING *"

[FraiseQL.mutations.createUser.arguments]
name = "String"
email = "String"

# ============================================================================
# SECURITY
# ============================================================================

[FraiseQL.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60

[FraiseQL.security.audit_logging]
enabled = true
log_level = "info"
log_mutations = true

# ============================================================================
# OBSERVERS
# ============================================================================

[FraiseQL.observers.userCreated]
entity = "User"
event = "INSERT"
description = "Send welcome email on user creation"

[[FraiseQL.observers.userCreated.actions]]
type = "email"
to = "{email}"
from = "welcome@example.com"
subject = "Welcome!"
body = "Thanks for signing up"

# ============================================================================
# CACHING
# ============================================================================

[FraiseQL.caching]
enabled = true
default_ttl_seconds = 300

[[FraiseQL.caching.rules]]
query = "users"
ttl_seconds = 600
invalidate_on = ["User"]
```text
<!-- Code example in TEXT -->

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
- [API Documentation](reference/README.md)
- [Examples](../examples/)
