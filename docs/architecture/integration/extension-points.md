<!-- Skip to main content -->
---
title: FraiseQL Extension Points: Plugins, Customization, and Integration Hooks
description: FraiseQL provides extension points for customization without modifying core framework. Developers can extend behavior through hooks, custom validators, authoriz
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# FraiseQL Extension Points: Plugins, Customization, and Integration Hooks

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Framework extension developers, plugin authors, integration engineers

---

## Executive Summary

FraiseQL provides extension points for customization without modifying core framework. Developers can extend behavior through hooks, custom validators, authorization rules, and metrics.

**Core principle**: Extensibility through composition, not modification. All extensions operate within compiled schema constraints.

---

## 1. Authorization Rule Extensions

### 1.1 Custom Authorization Rules

Define custom rules beyond built-in:

```python
<!-- Code example in Python -->
@FraiseQL.authorization_rule(name="published_or_author")
def rule_published_or_author(
    resource: Any,
    user_context: UserContext
) -> bool:
    """Published posts or posts by current user"""
    return (
        resource.published == True
        or resource.author_id == user_context.user_id
    )

@FraiseQL.authorization_rule(name="team_member")
def rule_team_member(
    resource: Any,
    user_context: UserContext
) -> bool:
    """User is member of resource's team"""
    return resource.team_id in user_context.team_ids

# Use in schema
@FraiseQL.type
class Post:
    @FraiseQL.authorize(rule="published_or_author")
    content: str

@FraiseQL.type
class Project:
    @FraiseQL.authorize(rule="team_member")
    budget: float
```text
<!-- Code example in TEXT -->

### 1.2 Complex Rule Logic

Rules can access database for context:

```python
<!-- Code example in Python -->
@FraiseQL.authorization_rule(name="department_lead_or_admin")
async def rule_department_lead(
    resource: Any,
    user_context: UserContext,
    db: Database  # Database connection provided
) -> bool:
    """User is department lead or admin"""
    if user_context.is_admin:
        return True

    # Query to check if user is lead of this department
    is_lead = await db.query_one(
        "SELECT COUNT(*) FROM tb_department_leads "
        "WHERE user_id = $1 AND department_id = $2",
        [user_context.user_id, resource.department_id]
    )
    return bool(is_lead)
```text
<!-- Code example in TEXT -->

### 1.3 Dynamic Rule Caching

Cache authorization decisions:

```python
<!-- Code example in Python -->
@FraiseQL.authorization_rule(
    name="expensive_rule",
    cache_ttl_seconds=300,
    cache_key_fields=["user_id", "resource_id"]
)
async def expensive_rule(resource, user_context, db):
    """Rule with expensive computation"""
    # Cached automatically for 5 minutes
    # Cache key: {user_id}:{resource_id}

    result = await db.query_expensive(...)
    return result
```text
<!-- Code example in TEXT -->

---

## 2. Validation Rule Extensions

### 2.1 Custom Validators

Define validation on input fields:

```python
<!-- Code example in Python -->
@FraiseQL.validator(name="email_validator")
def validate_email(value: str) -> None:
    """Validate email format and domain"""
    if not "@" in value:
        raise ValidationError("Invalid email format")

    domain = value.split("@")[1]
    if domain not in ["company.com", "trusted-partner.com"]:
        raise ValidationError(f"Email domain {domain} not allowed")

@FraiseQL.validator(name="password_strength")
def validate_password(value: str) -> None:
    """Validate password meets security requirements"""
    if len(value) < 12:
        raise ValidationError("Password must be at least 12 characters")

    if not any(c.isupper() for c in value):
        raise ValidationError("Password must contain uppercase letter")

    if not any(c.isdigit() for c in value):
        raise ValidationError("Password must contain digit")

# Use in schema
@FraiseQL.type
class User:
    @FraiseQL.validate(rule="email_validator")
    email: str

    @FraiseQL.mutation
    @FraiseQL.validate(rule="password_strength")
    def update_password(self, new_password: str) -> bool:
        """Update user password"""
        pass
```text
<!-- Code example in TEXT -->

### 2.2 Async Validators

Validators can query database:

```python
<!-- Code example in Python -->
@FraiseQL.validator(name="unique_email")
async def validate_unique_email(
    value: str,
    db: Database
) -> None:
    """Ensure email is unique in database"""
    existing = await db.query_one(
        "SELECT id FROM tb_user WHERE email = $1",
        [value]
    )

    if existing:
        raise ValidationError(f"Email {value} already exists")

# Use in mutation
@FraiseQL.mutation
def create_user(input: CreateUserInput) -> User:
    """Create user with unique email validation"""
    # @unique_email validator runs during input validation
    pass
```text
<!-- Code example in TEXT -->

---

## 3. Lifecycle Hooks

### 3.1 Query Hooks

Execute code before/after queries:

```python
<!-- Code example in Python -->
@FraiseQL.hook(event="query.before_execution")
async def log_query_start(
    query_name: str,
    variables: dict,
    user_context: UserContext
) -> None:
    """Log query execution start"""
    logger.info(
        f"Query {query_name} started",
        extra={
            "user_id": user_context.user_id,
            "variables": variables
        }
    )

@FraiseQL.hook(event="query.after_execution")
async def log_query_end(
    query_name: str,
    duration_ms: float,
    error: Exception | None,
    user_context: UserContext
) -> None:
    """Log query execution end"""
    if error:
        logger.error(
            f"Query {query_name} failed: {error}",
            extra={"user_id": user_context.user_id}
        )
    else:
        logger.info(
            f"Query {query_name} completed in {duration_ms}ms",
            extra={"user_id": user_context.user_id}
        )
```text
<!-- Code example in TEXT -->

### 3.2 Mutation Hooks

Execute code before/after mutations:

```python
<!-- Code example in Python -->
@FraiseQL.hook(event="mutation.before_execution")
async def audit_mutation_intent(
    mutation_name: str,
    input_data: dict,
    user_context: UserContext
) -> None:
    """Log intention to perform mutation"""
    logger.info(
        f"Mutation {mutation_name} requested",
        extra={
            "user_id": user_context.user_id,
            "mutation": mutation_name,
            "data_summary": summarize_data(input_data)
        }
    )

@FraiseQL.hook(event="mutation.after_execution")
async def handle_mutation_side_effects(
    mutation_name: str,
    result: Any,
    user_context: UserContext
) -> None:
    """Handle side effects after mutation"""
    if mutation_name == "DeleteUser":
        # Trigger cleanup jobs
        await trigger_user_cleanup(result.user_id)
    elif mutation_name == "CreateOrder":
        # Send confirmation email
        await send_order_confirmation(result.order_id)
```text
<!-- Code example in TEXT -->

### 3.3 Subscription Hooks

Execute code on subscription lifecycle:

```python
<!-- Code example in Python -->
@FraiseQL.hook(event="subscription.connected")
async def on_subscription_connected(
    subscription_id: str,
    subscription_name: str,
    user_context: UserContext
) -> None:
    """Handle new subscription connection"""
    logger.info(
        f"Subscription {subscription_name} connected",
        extra={
            "subscription_id": subscription_id,
            "user_id": user_context.user_id
        }
    )

    # Track active subscriptions
    await metrics.gauge("active_subscriptions", increment=1)

@FraiseQL.hook(event="subscription.disconnected")
async def on_subscription_disconnected(
    subscription_id: str,
    subscription_name: str,
    reason: str
) -> None:
    """Handle subscription disconnection"""
    logger.info(
        f"Subscription {subscription_name} disconnected: {reason}",
        extra={"subscription_id": subscription_id}
    )

    await metrics.gauge("active_subscriptions", increment=-1)
```text
<!-- Code example in TEXT -->

### 3.4 Error Hooks

Execute code on errors:

```python
<!-- Code example in Python -->
@FraiseQL.hook(event="error.occurred")
async def handle_error(
    error: Exception,
    error_code: str,
    operation_type: str,
    user_context: UserContext
) -> None:
    """Handle any error in operation"""

    # Log authorization errors separately
    if error_code.startswith("E_AUTH_"):
        logger.warning(
            f"Authorization error: {error_code}",
            extra={"user_id": user_context.user_id}
        )
        await notify_security(error_code, user_context.user_id)

    # Alert on database errors
    elif error_code.startswith("E_DB_"):
        logger.error(
            f"Database error: {error_code}",
            extra={"error": str(error)}
        )
        await alert_oncall("Database error detected")
```text
<!-- Code example in TEXT -->

---

## 4. Custom Metrics

### 4.1 Counter Metrics

Track occurrences:

```python
<!-- Code example in Python -->
@FraiseQL.metric(name="user_created", type="counter")
def track_user_creation():
    """Track user creation count"""
    # Auto-incremented on mutation
    pass

# Use in code
@FraiseQL.hook(event="mutation.after_execution")
async def track_mutations(mutation_name, result):
    if mutation_name == "CreateUser":
        metrics.increment("user_created")
        metrics.increment("mutations_total", labels={"type": "create"})
```text
<!-- Code example in TEXT -->

### 4.2 Gauge Metrics

Track instantaneous values:

```python
<!-- Code example in Python -->
@FraiseQL.metric(name="active_sessions", type="gauge")
async def update_active_sessions():
    """Track active user sessions"""
    count = await db.query_one(
        "SELECT COUNT(*) FROM tb_session WHERE active = true"
    )
    metrics.set("active_sessions", count)

# Periodic update
@FraiseQL.schedule(interval_seconds=60)
async def refresh_gauge_metrics():
    await update_active_sessions()
```text
<!-- Code example in TEXT -->

### 4.3 Histogram Metrics

Track distributions:

```python
<!-- Code example in Python -->
@FraiseQL.metric(name="query_duration_ms", type="histogram")
async def track_query_latency(duration_ms: float):
    """Track query latency distribution"""
    metrics.histogram("query_duration_ms", duration_ms)

# Use in hook
@FraiseQL.hook(event="query.after_execution")
async def record_latency(duration_ms, query_name):
    metrics.histogram(
        "query_duration_ms",
        duration_ms,
        labels={"operation": query_name}
    )
```text
<!-- Code example in TEXT -->

---

## 5. Custom Scalars

### 5.1 Custom Scalar Types

Define domain-specific types:

```python
<!-- Code example in Python -->
@FraiseQL.scalar(name="Email")
class EmailScalar:
    """Custom Email scalar with validation"""

    @staticmethod
    def serialize(value: str) -> str:
        """Convert to JSON"""
        return value

    @staticmethod
    def parse_value(value: Any) -> str:
        """Parse from JSON"""
        if not isinstance(value, str):
            raise ValueError("Email must be string")
        if "@" not in value:
            raise ValueError("Invalid email format")
        return value

    @staticmethod
    def parse_literal(ast) -> str:
        """Parse from GraphQL query literal"""
        if ast.value == "null":
            return None
        return EmailScalar.parse_value(ast.value)

@FraiseQL.scalar(name="Money")
class MoneyScalar:
    """Custom Money scalar (amount + currency)"""

    @staticmethod
    def serialize(value: dict) -> dict:
        return {"amount": value.amount, "currency": value.currency}

    @staticmethod
    def parse_value(value: dict) -> dict:
        return {
            "amount": Decimal(str(value["amount"])),
            "currency": value["currency"]
        }

# Use in schema
@FraiseQL.type
class User:
    email: Email  # Custom scalar

@FraiseQL.type
class Order:
    total: Money  # Custom scalar
```text
<!-- Code example in TEXT -->

---

## 6. Custom Directives

### 6.1 Field Directives

Apply behavior to fields:

```python
<!-- Code example in Python -->
@FraiseQL.directive(name="uppercase")
def uppercase_directive(value: str) -> str:
    """Convert field value to uppercase"""
    return value.upper() if value else value

@FraiseQL.directive(name="redact")
def redact_directive(value: str) -> str:
    """Redact sensitive value"""
    if len(value) > 4:
        return value[:2] + "****" + value[-2:]
    return "****"

# Use in schema
@FraiseQL.type
class User:
    @uppercase_directive
    name: str

    @redact_directive
    ssn: str
```text
<!-- Code example in TEXT -->

### 6.2 Query Directives

Apply behavior to queries:

```python
<!-- Code example in Python -->
@FraiseQL.directive(name="cache")
def cache_directive(result: Any, ttl_seconds: int) -> Any:
    """Cache query result"""
    # Framework handles caching
    return result

@FraiseQL.directive(name="rateLimit")
def rate_limit_directive(
    user_context: UserContext,
    limit: int,
    window_seconds: int
) -> None:
    """Rate limit query per user"""
    key = f"ratelimit:{user_context.user_id}"
    current = cache.get(key) or 0

    if current >= limit:
        raise RateLimitError(f"Rate limit exceeded: {limit}/{window_seconds}s")

    cache.increment(key, 1, ttl=window_seconds)

# Use in query
@FraiseQL.query
@cache_directive(ttl_seconds=300)
@rate_limit_directive(limit=100, window_seconds=60)
def get_user_posts(user_id: ID) -> [Post]:
    """Get user's posts (cached, rate-limited)"""
    pass
```text
<!-- Code example in TEXT -->

---

## 7. Transform Hooks

### 7.1 Input Transform

Transform input before validation:

```python
<!-- Code example in Python -->
@FraiseQL.transform(event="input.before_validation")
def normalize_email(input_data: dict) -> dict:
    """Normalize email to lowercase"""
    if "email" in input_data:
        input_data["email"] = input_data["email"].lower().strip()
    return input_data

@FraiseQL.transform(event="input.before_validation")
def sanitize_text(input_data: dict) -> dict:
    """Sanitize text inputs to prevent XSS"""
    for field in ["title", "content", "description"]:
        if field in input_data and isinstance(input_data[field], str):
            input_data[field] = sanitize_html(input_data[field])
    return input_data
```text
<!-- Code example in TEXT -->

### 7.2 Output Transform

Transform response before sending:

```python
<!-- Code example in Python -->
@FraiseQL.transform(event="response.before_sending")
def add_metadata(response: dict) -> dict:
    """Add request metadata to response"""
    response["_metadata"] = {
        "timestamp": now_iso(),
        "version": "2.0.0"
    }
    return response

@FraiseQL.transform(event="response.before_sending")
def redact_sensitive(response: dict) -> dict:
    """Redact sensitive fields from response"""
    if "user" in response.get("data", {}):
        user = response["data"]["user"]
        if "password_hash" in user:
            del user["password_hash"]
    return response
```text
<!-- Code example in TEXT -->

---

## 8. Middleware Extensions

### 8.1 Request Middleware

Process requests:

```python
<!-- Code example in Python -->
@FraiseQL.middleware(type="request")
async def add_request_id(request, next_handler):
    """Add unique request ID"""
    request.id = generate_uuid()
    logger.info(f"Request {request.id} started")

    try:
        response = await next_handler(request)
        logger.info(f"Request {request.id} completed")
        return response
    except Exception as e:
        logger.error(f"Request {request.id} failed: {e}")
        raise

@FraiseQL.middleware(type="request")
async def extract_user_context(request, next_handler):
    """Extract user from token"""
    token = extract_bearer_token(request)
    if token:
        request.user_context = verify_token(token)
    return await next_handler(request)
```text
<!-- Code example in TEXT -->

### 8.2 Response Middleware

Process responses:

```python
<!-- Code example in Python -->
@FraiseQL.middleware(type="response")
async def add_cache_headers(request, response, next_handler):
    """Add cache control headers"""
    if is_cacheable_query(request):
        response.headers["Cache-Control"] = "public, max-age=300"
    return response

@FraiseQL.middleware(type="response")
async def compress_response(request, response, next_handler):
    """Compress response if large"""
    if len(response.body) > 1024:
        response.body = gzip_compress(response.body)
        response.headers["Content-Encoding"] = "gzip"
    return response
```text
<!-- Code example in TEXT -->

---

## 9. Database Extensions

### 9.1 Custom Database Functions

Call custom database functions from queries:

```python
<!-- Code example in Python -->
@FraiseQL.database_function(name="search_full_text")
def search_full_text(
    db: Database,
    query: str,
    table: str
) -> list:
    """Full-text search using database function"""
    return db.query(
        f"SELECT * FROM {table} WHERE search_vector @@ to_tsquery($1)",
        [query]
    )

# Use in schema
@FraiseQL.query
def search_posts(query: str) -> [Post]:
    """Search posts by full-text"""
    return search_full_text(query, "tb_post")
```text
<!-- Code example in TEXT -->

### 9.2 Custom Database Views

Define custom materialized views:

```python
<!-- Code example in Python -->
@FraiseQL.view(name="v_user_stats")
def create_user_stats_view(db: Database) -> str:
    """Create materialized view with user statistics"""
    return """
    CREATE MATERIALIZED VIEW IF NOT EXISTS v_user_stats AS
    SELECT
        u.id,
        u.username,
        COUNT(p.id) as post_count,
        COUNT(DISTINCT c.id) as comment_count,
        MAX(p.created_at) as last_post_date
    FROM tb_user u
    LEFT JOIN tb_post p ON u.id = p.author_id
    LEFT JOIN tb_comment c ON u.id = c.author_id
    GROUP BY u.id, u.username
    WITH DATA;

    CREATE INDEX idx_user_stats_id ON v_user_stats(id);
    """
```text
<!-- Code example in TEXT -->

---

## 10. Extension Configuration

### 10.1 Enable/Disable Extensions

Control which extensions are active:

```python
<!-- Code example in Python -->
FraiseQL.extensions.configure({
    "authorization": {
        "custom_rules": True,
        "enable_rules": [
            "published_or_author",
            "team_member",
            "department_lead"
        ]
    },
    "validators": {
        "enabled": True,
        "enable_validators": [
            "email_validator",
            "password_strength",
            "unique_email"
        ]
    },
    "hooks": {
        "enabled": True,
        "enable_hooks": [
            "query.before_execution",
            "mutation.after_execution",
            "error.occurred"
        ]
    },
    "metrics": {
        "enabled": True,
        "custom_metrics": True
    },
    "middleware": {
        "enabled": True,
        "order": [
            "add_request_id",
            "extract_user_context",
            "rate_limiting"
        ]
    }
})
```text
<!-- Code example in TEXT -->

### 10.2 Extension Namespace

Organize extensions:

```python
<!-- Code example in Python -->
# Define extension namespace
class CustomExtensions:
    @FraiseQL.authorization_rule(name="my_rule_1")
    def rule_1(resource, user_context):
        pass

    @FraiseQL.validator(name="my_validator_1")
    def validator_1(value):
        pass

    @FraiseQL.hook(event="query.before_execution")
    async def on_query_start(query_name, variables, user_context):
        pass

# Register namespace
FraiseQL.extensions.register(CustomExtensions)
```text
<!-- Code example in TEXT -->

---

## 11. Best Practices

### 11.1 Extension Development

**DO:**

- ✅ Keep extensions focused (single responsibility)
- ✅ Use async/await for I/O operations
- ✅ Cache expensive computations
- ✅ Log extension execution (for debugging)
- ✅ Test extensions in isolation
- ✅ Document extension behavior
- ✅ Use strong typing (type hints)

**DON'T:**

- ❌ Modify user context (read-only)
- ❌ Perform long-running operations in hooks (use async)
- ❌ Bypass authorization checks
- ❌ Log sensitive data (email, password, tokens)
- ❌ Assume extension execution order
- ❌ Create side effects in validators (should be pure)

### 11.2 Performance Considerations

```python
<!-- Code example in Python -->
# ❌ SLOW: Complex rule evaluated for every request
@FraiseQL.authorization_rule(name="slow_rule")
async def slow_rule(resource, user_context, db):
    # Database query for every field access
    result = await db.query_expensive(...)
    return result

# ✅ FAST: Rule with caching
@FraiseQL.authorization_rule(
    name="fast_rule",
    cache_ttl_seconds=300
)
async def fast_rule(resource, user_context, db):
    # Cached for 5 minutes
    result = await db.query_expensive(...)
    return result
```text
<!-- Code example in TEXT -->

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL extensions enable powerful customization while maintaining framework constraints and consistency guarantees.
