# Production Security

Comprehensive security guide for production FraiseQL deployments: SQL injection prevention, query complexity limits, rate limiting, CORS, authentication, PII handling, and compliance patterns.

## Overview

Production security requires defense in depth: multiple layers of protection from the network edge to the database, with continuous monitoring and incident response.

**Security Layers:**
- SQL injection prevention (parameterized queries)
- Query complexity analysis
- Rate limiting
- CORS configuration
- Authentication & authorization
- Sensitive data handling
- Audit logging
- Compliance (GDPR, SOC2)

## Table of Contents

- [SQL Injection Prevention](#sql-injection-prevention)
- [Query Complexity Limits](#query-complexity-limits)
- [Rate Limiting](#rate-limiting)
- [CORS Configuration](#cors-configuration)
- [Authentication Security](#authentication-security)
- [Sensitive Data Handling](#sensitive-data-handling)
- [Audit Logging](#audit-logging)
- [Compliance](#compliance)

## SQL Injection Prevention

### Parameterized Queries

FraiseQL uses parameterized queries exclusively:

```python
from fraiseql import type, query, mutation, input, field

# SAFE: Parameterized query
async def get_user(user_id: str) -> User:
    async with db.connection() as conn:
        result = await conn.execute(
            "SELECT * FROM users WHERE id = $1",
            user_id  # Automatically escaped
        )
        return result.fetchone()

# UNSAFE: String interpolation (never do this!)
# async def get_user_unsafe(user_id: str) -> User:
#     query = f"SELECT * FROM users WHERE id = '{user_id}'"
#     result = await conn.execute(query)  # VULNERABLE
```

### Input Validation

```python
from fraiseql import type, query, mutation, input, field

from fraiseql.security import InputValidator, ValidationResult

class UserInputValidator:
    """Validate user inputs."""

    @staticmethod
    def validate_user_id(user_id: str) -> ValidationResult:
        """Validate UUID format."""
        import uuid

        try:
            uuid.UUID(user_id)
            return ValidationResult(valid=True)
        except ValueError:
            return ValidationResult(
                valid=False,
                error="Invalid user ID format"
            )

    @staticmethod
    def validate_email(email: str) -> ValidationResult:
        """Validate email format."""
        import re

        pattern = r'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'
        if re.match(pattern, email):
            return ValidationResult(valid=True)
        else:
            return ValidationResult(
                valid=False,
                error="Invalid email format"
            )

# Usage in resolver
@mutation
async def update_user(info, user_id: str, email: str) -> User:
    # Validate inputs
    user_id_valid = UserInputValidator.validate_user_id(user_id)
    if not user_id_valid.valid:
        raise ValueError(user_id_valid.error)

    email_valid = UserInputValidator.validate_email(email)
    if not email_valid.valid:
        raise ValueError(email_valid.error)

    # Safe to proceed
    return await update_user_email(user_id, email)
```

### GraphQL Injection Prevention

```python
from graphql import parse, validate

def sanitize_graphql_query(query: str) -> str:
    """Validate GraphQL query syntax."""
    try:
        # Parse to AST (validates syntax)
        document = parse(query)

        # Validate against schema
        errors = validate(schema, document)
        if errors:
            raise ValueError(f"Invalid query: {errors}")

        return query

    except Exception as e:
        raise ValueError(f"Query validation failed: {e}")
```

## Query Complexity Limits

### Complexity Analysis

```python
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    # Query complexity limits
    complexity_enabled=True,
    complexity_max_score=1000,
    complexity_max_depth=10,
    complexity_default_list_size=10,
    # Field-specific multipliers
    complexity_field_multipliers={
        "users": 2,  # Expensive field
        "orders": 3,
        "analytics": 10
    }
)
```

### Depth Limiting

```python
from graphql import GraphQLError

def enforce_max_depth(document, max_depth: int = 10):
    """Prevent excessively nested queries."""
    from graphql import visit

    current_depth = 0

    def enter_field(node, key, parent, path, ancestors):
        nonlocal current_depth
        depth = len([a for a in ancestors if hasattr(a, "kind") and a.kind == "field"])

        if depth > max_depth:
            raise GraphQLError(
                f"Query depth {depth} exceeds maximum {max_depth}",
                extensions={"code": "MAX_DEPTH_EXCEEDED"}
            )

    visit(document, {"Field": {"enter": enter_field}})
```

### Cost Analysis

```python
from fraiseql.analysis.complexity import calculate_query_cost

@app.middleware("http")
async def query_cost_middleware(request: Request, call_next):
    if request.url.path != "/graphql":
        return await call_next(request)

    body = await request.json()
    query = body.get("query", "")

    # Calculate cost
    cost = calculate_query_cost(query, schema)

    # Reject expensive queries
    if cost > 1000:
        return Response(
            content=json.dumps({
                "errors": [{
                    "message": f"Query cost {cost} exceeds limit 1000",
                    "extensions": {"code": "QUERY_TOO_EXPENSIVE"}
                }]
            }),
            status_code=400,
            media_type="application/json"
        )

    return await call_next(request)
```

## Rate Limiting

### Redis-Based Rate Limiting

```python
from fraiseql.security import (
    setup_rate_limiting,
    RateLimitRule,
    RateLimit,
    RedisRateLimitStore
)
import redis.asyncio as redis

# Redis client
redis_client = redis.from_url("redis://localhost:6379/0")

# Rate limit rules
rate_limits = [
    # GraphQL endpoint
    RateLimitRule(
        path_pattern="/graphql",
        rate_limit=RateLimit(requests=100, window=60),  # 100/min
        message="GraphQL rate limit exceeded"
    ),
    # Authentication endpoints
    RateLimitRule(
        path_pattern="/auth/login",
        rate_limit=RateLimit(requests=5, window=300),  # 5 per 5 min
        message="Too many login attempts"
    ),
    RateLimitRule(
        path_pattern="/auth/register",
        rate_limit=RateLimit(requests=3, window=3600),  # 3 per hour
        message="Too many registration attempts"
    ),
    # Mutations
    RateLimitRule(
        path_pattern="/graphql",
        rate_limit=RateLimit(requests=20, window=60),  # 20/min for mutations
        http_methods=["POST"],
        message="Mutation rate limit exceeded"
    )
]

# Setup rate limiting
setup_rate_limiting(
    app=app,
    redis_client=redis_client,
    custom_rules=rate_limits
)
```

### Per-User Rate Limiting

```python
from fraiseql.security import GraphQLRateLimiter

class PerUserRateLimiter:
    """Rate limit per authenticated user."""

    def __init__(self, redis_client):
        self.redis = redis_client

    async def check_rate_limit(
        self,
        user_id: str,
        limit: int = 100,
        window: int = 60
    ) -> bool:
        """Check if user is within rate limit."""
        key = f"rate_limit:user:{user_id}"
        current = await self.redis.incr(key)

        if current == 1:
            await self.redis.expire(key, window)

        if current > limit:
            return False

        return True

@app.middleware("http")
async def user_rate_limit_middleware(request: Request, call_next):
    if not hasattr(request.state, "user"):
        return await call_next(request)

    user_id = request.state.user.user_id

    limiter = PerUserRateLimiter(redis_client)
    allowed = await limiter.check_rate_limit(user_id)

    if not allowed:
        return Response(
            content=json.dumps({
                "errors": [{
                    "message": "Rate limit exceeded for user",
                    "extensions": {"code": "USER_RATE_LIMIT_EXCEEDED"}
                }]
            }),
            status_code=429,
            media_type="application/json"
        )

    return await call_next(request)
```

## CORS Configuration

### Production CORS Setup

```python
from fraiseql.fastapi.config import FraiseQLConfig

config = FraiseQLConfig(
    database_url="postgresql://...",
    # CORS - disabled by default, configure explicitly
    cors_enabled=True,
    cors_origins=[
        "https://app.yourapp.com",
        "https://www.yourapp.com",
        # NEVER use "*" in production
    ],
    cors_methods=["GET", "POST"],
    cors_headers=[
        "Content-Type",
        "Authorization",
        "X-Request-ID"
    ]
)
```

### Custom CORS Middleware

```python
from starlette.middleware.cors import CORSMiddleware

app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "https://app.yourapp.com",
        "https://www.yourapp.com"
    ],
    allow_credentials=True,
    allow_methods=["GET", "POST", "OPTIONS"],
    allow_headers=[
        "Content-Type",
        "Authorization",
        "X-Request-ID",
        "X-Correlation-ID"
    ],
    expose_headers=["X-Request-ID"],
    max_age=3600  # Cache preflight for 1 hour
)
```

## Authentication Security

### Token Security

```python
from fraiseql import type, query, mutation, input, field

# JWT configuration
from fraiseql.auth import CustomJWTProvider

auth_provider = CustomJWTProvider(
    secret_key=os.getenv("JWT_SECRET_KEY"),  # NEVER hardcode
    algorithm="HS256",
    issuer="https://yourapp.com",
    audience="https://api.yourapp.com"
)

# Token expiration
ACCESS_TOKEN_TTL = 3600  # 1 hour
REFRESH_TOKEN_TTL = 2592000  # 30 days

# Token rotation
@mutation
async def refresh_access_token(info, refresh_token: str) -> dict:
    """Rotate access token using refresh token."""
    # Validate refresh token
    payload = await auth_provider.validate_token(refresh_token)

    # Check token type
    if payload.get("token_type") != "refresh":
        raise ValueError("Invalid token type")

    # Generate new access token
    new_access_token = generate_access_token(
        user_id=payload["sub"],
        ttl=ACCESS_TOKEN_TTL
    )

    # Optionally rotate refresh token too
    new_refresh_token = generate_refresh_token(
        user_id=payload["sub"],
        ttl=REFRESH_TOKEN_TTL
    )

    # Revoke old refresh token
    await revocation_service.revoke_token(payload)

    return {
        "access_token": new_access_token,
        "refresh_token": new_refresh_token,
        "token_type": "bearer"
    }
```

### Password Security

```python
import bcrypt

class PasswordHasher:
    """Secure password hashing with bcrypt."""

    @staticmethod
    def hash_password(password: str) -> str:
        """Hash password with bcrypt."""
        salt = bcrypt.gensalt(rounds=12)
        hashed = bcrypt.hashpw(password.encode(), salt)
        return hashed.decode()

    @staticmethod
    def verify_password(password: str, hashed: str) -> bool:
        """Verify password against hash."""
        return bcrypt.checkpw(password.encode(), hashed.encode())

    @staticmethod
    def validate_password_strength(password: str) -> bool:
        """Validate password meets security requirements."""
        if len(password) < 12:
            return False
        if not any(c.isupper() for c in password):
            return False
        if not any(c.islower() for c in password):
            return False
        if not any(c.isdigit() for c in password):
            return False
        if not any(c in "!@#$%^&*()-_=+[]{}|;:,.<>?" for c in password):
            return False
        return True
```

## Sensitive Data Handling

### PII Protection

```python
from fraiseql import type, query, mutation, input, field

from dataclasses import dataclass

@dataclass
class User:
    """User with PII protection."""
    id: UUID
    email: str
    name: str
    _ssn: str | None = None  # Private field
    _credit_card: str | None = None

    @property
    def ssn_masked(self) -> str | None:
        """Return masked SSN."""
        if not self._ssn:
            return None
        return f"***-**-{self._ssn[-4:]}"

    @property
    def credit_card_masked(self) -> str | None:
        """Return masked credit card."""
        if not self._credit_card:
            return None
        return f"****-****-****-{self._credit_card[-4:]}"

# GraphQL type
@type_
class UserGQL:
    id: UUID
    email: str
    name: str

    # Only admins can see full SSN
    @authorize_field(lambda obj, info: info.context["user"].has_role("admin"))
    async def ssn(self) -> str | None:
        return self._ssn

    # Everyone sees masked version
    async def ssn_masked(self) -> str | None:
        return self.ssn_masked
```

### Data Encryption

```python
from cryptography.fernet import Fernet
import os

class FieldEncryption:
    """Encrypt sensitive database fields."""

    def __init__(self):
        key = os.getenv("ENCRYPTION_KEY")  # Store in secrets manager
        self.cipher = Fernet(key.encode())

    def encrypt(self, value: str) -> str:
        """Encrypt field value."""
        return self.cipher.encrypt(value.encode()).decode()

    def decrypt(self, encrypted: str) -> str:
        """Decrypt field value."""
        return self.cipher.decrypt(encrypted.encode()).decode()

# Usage
encryptor = FieldEncryption()

# Store encrypted
encrypted_ssn = encryptor.encrypt("123-45-6789")
await conn.execute(
    "INSERT INTO users (id, ssn_encrypted) VALUES ($1, $2)",
    user_id, encrypted_ssn
)

# Retrieve and decrypt
result = await conn.execute("SELECT ssn_encrypted FROM users WHERE id = $1", user_id)
encrypted = result.fetchone()["ssn_encrypted"]
ssn = encryptor.decrypt(encrypted)
```

## Audit Logging

### Security Event Logging

```python
from fraiseql import type, query, mutation, input, field

from fraiseql.audit import get_security_logger, SecurityEventType, SecurityEventSeverity

security_logger = get_security_logger()

# Log authentication events
@mutation
async def login(info, username: str, password: str) -> dict:
    try:
        user = await authenticate_user(username, password)

        security_logger.log_auth_success(
            user_id=user.id,
            user_email=user.email,
            metadata={"ip": info.context["request"].client.host}
        )

        return {"token": generate_token(user)}

    except AuthenticationError as e:
        security_logger.log_auth_failure(
            reason=str(e),
            metadata={
                "username": username,
                "ip": info.context["request"].client.host
            }
        )
        raise

# Log data access
@query
@requires_permission("pii:read")
async def get_user_pii(info, user_id: str) -> UserPII:
    user = await fetch_user_pii(user_id)

    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.DATA_ACCESS,
            severity=SecurityEventSeverity.INFO,
            user_id=info.context["user"].user_id,
            metadata={
                "accessed_user": user_id,
                "pii_fields": ["ssn", "credit_card"]
            }
        )
    )

    return user
```

### Entity Change Log

```python
from fraiseql import type, query, mutation, input, field

# Automatic audit trail via PostgreSQL trigger
# See advanced/event-sourcing.md for complete implementation

@mutation
async def update_order_status(info, order_id: str, status: str) -> Order:
    """Update order status - automatically logged."""
    user_id = info.context["user"].user_id

    async with db.connection() as conn:
        # Set user context for trigger
        await conn.execute(
            "SET LOCAL app.current_user_id = $1",
            user_id
        )

        # Update (trigger logs before/after state)
        await conn.execute(
            "UPDATE orders SET status = $1 WHERE id = $2",
            status, order_id
        )

    return await fetch_order(order_id)
```

## Compliance

### GDPR Compliance

```python
from fraiseql import type, query, mutation, input, field

@mutation
@requires_auth
async def export_my_data(info) -> str:
    """GDPR: Export all user data."""
    user_id = info.context["user"].user_id

    # Gather all user data
    data = {
        "user": await fetch_user(user_id),
        "orders": await fetch_user_orders(user_id),
        "activity": await fetch_user_activity(user_id),
        "consents": await fetch_user_consents(user_id)
    }

    # Log export
    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.DATA_EXPORT,
            severity=SecurityEventSeverity.INFO,
            user_id=user_id
        )
    )

    return json.dumps(data, default=str)

@mutation
@requires_auth
async def delete_my_account(info) -> bool:
    """GDPR: Right to be forgotten."""
    user_id = info.context["user"].user_id

    async with db.connection() as conn:
        async with conn.transaction():
            # Anonymize or delete data
            await conn.execute(
                "UPDATE users SET email = $1, name = $2, deleted_at = NOW() WHERE id = $3",
                f"deleted-{user_id}@deleted.com",
                "Deleted User",
                user_id
            )

            # Delete related data
            await conn.execute("DELETE FROM user_sessions WHERE user_id = $1", user_id)
            await conn.execute("DELETE FROM user_consents WHERE user_id = $1", user_id)

    # Log deletion
    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.DATA_DELETION,
            severity=SecurityEventSeverity.WARNING,
            user_id=user_id
        )
    )

    return True
```

### SOC2 Controls

```python
from fraiseql import type, query, mutation, input, field

# Access control matrix
ROLE_PERMISSIONS = {
    "user": ["orders:read:self", "profile:write:self"],
    "manager": ["orders:read:team", "users:read:team"],
    "admin": ["admin:all"]
}

# Audit all administrative actions
@mutation
@requires_role("admin")
async def admin_update_user(info, user_id: str, data: dict) -> User:
    """Admin action - fully audited."""
    admin_user = info.context["user"]

    # Log before change
    before_state = await fetch_user(user_id)

    # Perform change
    updated_user = await update_user(user_id, data)

    # Log after change
    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.ADMIN_ACTION,
            severity=SecurityEventSeverity.WARNING,
            user_id=admin_user.user_id,
            metadata={
                "action": "update_user",
                "target_user": user_id,
                "before": before_state,
                "after": updated_user,
                "changed_fields": list(data.keys())
            }
        )
    )

    return updated_user
```

## Next Steps

- [Security Example](../../examples/security/) - Complete security implementation
- [Authentication](../advanced/authentication.md) - Authentication patterns
- [Monitoring](monitoring.md) - Security monitoring
- [Deployment](deployment.md) - Secure deployment
- [Audit Logging](../advanced/event-sourcing.md) - Complete audit trails
